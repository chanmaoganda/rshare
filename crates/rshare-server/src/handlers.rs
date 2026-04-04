use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
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

    if let Err(e) = state.storage.save(id, &data).await {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}"));
    }

    if let Err(e) = state.db.insert(&meta) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}"));
    }

    tracing::info!("Uploaded file: {} ({} bytes)", file_name, data.len());

    (
        StatusCode::CREATED,
        Json(UploadResponse {
            id,
            name: meta.name,
            size: meta.size,
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

pub async fn download(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let meta = match state.db.get(id) {
        Ok(Some(m)) => m,
        Ok(None) => return err(StatusCode::NOT_FOUND, "File not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    };

    let data = match state.storage.read(id).await {
        Ok(d) => d,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}")),
    };

    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", meta.name),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .unwrap()
}

pub async fn delete_file(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
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

pub async fn share_download(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    let meta = match state.db.get_by_share_token(&token) {
        Ok(Some(m)) => m,
        Ok(None) => return err(StatusCode::NOT_FOUND, "Invalid share link"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    };

    let data = match state.storage.read(meta.id).await {
        Ok(d) => d,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("Storage error: {e}")),
    };

    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", meta.name),
        )
        .header(header::CONTENT_LENGTH, data.len())
        .body(Body::from(data))
        .unwrap()
}
