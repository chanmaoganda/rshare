use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use chrono::Utc;
use rshare_common::{ErrorResponse, FileListResponse, FileMetadata, UploadResponse};
use uuid::Uuid;

use crate::AppState;

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorResponse { error: msg.into() })).into_response()
}

pub async fn upload(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return err(StatusCode::BAD_REQUEST, "No file field in upload"),
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("Multipart error: {e}")),
    };

    let file_name = field
        .file_name()
        .unwrap_or("unnamed")
        .to_string();

    let data = match field.bytes().await {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_REQUEST, format!("Failed to read file: {e}")),
    };

    let id = Uuid::new_v4();
    let meta = FileMetadata {
        id,
        name: file_name.clone(),
        size: data.len() as u64,
        uploaded_at: Utc::now(),
        share_token: None,
    };

    let delete_token = Uuid::new_v4().to_string().replace('-', "")[..16].to_string();

    if let Err(e) = state.storage.save(id, &data).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}"));
    }

    if let Err(e) = state.db.insert(&meta, &delete_token) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}"));
    }

    tracing::info!("Uploaded file: {} ({} bytes)", file_name, data.len());

    (
        StatusCode::CREATED,
        Json(UploadResponse {
            id,
            name: meta.name,
            size: meta.size,
            delete_token,
        }),
    )
        .into_response()
}

pub async fn list_files(State(state): State<AppState>) -> Response {
    match state.db.list() {
        Ok(files) => Json(FileListResponse { files }).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }
}

pub async fn get_file(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match state.db.get(id) {
        Ok(Some(meta)) => Json(meta).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }
}

pub async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let meta = match state.db.get(id) {
        Ok(Some(m)) => m,
        Ok(None) => return err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    };

    let data = match state.storage.read(id).await {
        Ok(d) => d,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}")),
    };

    serve_range(data, &meta.name, &headers)
}

pub async fn delete_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    // Check admin token first
    let is_admin = state
        .admin_token
        .as_ref()
        .is_some_and(|expected| provided.as_deref() == Some(expected));

    // If not admin, check uploader's delete token
    if !is_admin {
        let file_delete_token = match state.db.get_delete_token(id) {
            Ok(Some(t)) => t,
            Ok(None) => return err(StatusCode::NOT_FOUND, "File not found"),
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
        };

        let has_delete_token = provided
            .as_deref()
            .is_some_and(|t| t == file_delete_token);

        if !has_delete_token {
            // If admin_token is configured, require auth; if not, also require delete token
            return err(StatusCode::UNAUTHORIZED, "Invalid or missing token. Use admin token or the file's delete token.");
        }
    }

    match state.db.delete(id) {
        Ok(true) => {
            let _ = state.storage.delete(id).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }
}

pub async fn share_create(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    // Check file exists
    match state.db.get(id) {
        Ok(Some(meta)) => {
            if let Some(token) = &meta.share_token {
                return Json(serde_json::json!({
                    "share_url": format!("/share/{}", token)
                }))
                .into_response();
            }
        }
        Ok(None) => return err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }

    let token = Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
    match state.db.set_share_token(id, &token) {
        Ok(true) => Json(serde_json::json!({
            "share_url": format!("/share/{}", token)
        }))
        .into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }
}

pub async fn share_download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Response {
    let meta = match state.db.get_by_share_token(&token) {
        Ok(Some(m)) => m,
        Ok(None) => return err(StatusCode::NOT_FOUND, "Invalid share link"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    };

    let data = match state.storage.read(meta.id).await {
        Ok(d) => d,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}")),
    };

    serve_range(data, &meta.name, &headers)
}

/// Serve file data with HTTP Range support for resumable downloads.
fn serve_range(data: Vec<u8>, filename: &str, headers: &HeaderMap) -> Response {
    let total = data.len();

    let range_start = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.split('-').next())
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);

    if range_start >= total {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{total}"))
            .body(Body::empty())
            .unwrap();
    }

    let sliced = &data[range_start..];
    let (status, content_range) = if range_start > 0 {
        (
            StatusCode::PARTIAL_CONTENT,
            Some(format!("bytes {range_start}-{}/{total}", total - 1)),
        )
    } else {
        (StatusCode::OK, None)
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .header(header::CONTENT_LENGTH, sliced.len())
        .header(header::ACCEPT_RANGES, "bytes");

    if let Some(cr) = content_range {
        builder = builder.header("Content-Range", cr);
    }

    builder.body(Body::from(sliced.to_vec())).unwrap()
}
