use axum::body::Body;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Json, Response};
use chrono::{Duration, Utc};
use rshare_common::{ErrorResponse, FileMetadata, UploadResponse};
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthContext, OptionalAuthContext};

fn sanitize_filename(raw: &str) -> String {
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .replace("..", "")
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .to_string();
    if name.is_empty() {
        "unnamed".to_string()
    } else if name.len() > 255 {
        // Find the last char boundary at or before 255 bytes to avoid panic on multibyte chars
        let mut end = 255;
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name[..end].to_string()
    } else {
        name
    }
}

fn content_disposition(filename: &str) -> String {
    // ASCII fallback: strip non-ASCII, escape quotes
    let ascii_name: String = filename
        .chars()
        .filter(|c| c.is_ascii() && *c != '"' && *c != '\\')
        .collect();
    // RFC 5987 percent-encoded UTF-8 filename
    let encoded: String = filename
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("attachment; filename=\"{ascii_name}\"; filename*=UTF-8''{encoded}")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorResponse { error: msg.into() })).into_response()
}

fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    for unit in UNITS {
        if size < 1024.0 {
            return format!("{size:.1} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.1} PB")
}

pub async fn upload(
    State(state): State<AppState>,
    auth: AuthContext,
    mut multipart: Multipart,
) -> Response {
    if let Err(resp) = auth.require_permission("upload") {
        return resp;
    }

    let _permit = match state.upload_semaphore.try_acquire() {
        Ok(permit) => permit,
        Err(_) => {
            return err(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many concurrent uploads, try again later",
            );
        }
    };

    tracing::debug!("Upload request received");

    let field = match multipart.next_field().await {
        Ok(Some(f)) => f,
        Ok(None) => return err(StatusCode::BAD_REQUEST, "No file field in upload"),
        Err(e) => {
            tracing::error!("Multipart parse error: {e:?}");
            return err(StatusCode::BAD_REQUEST, format!("Multipart error: {e}"));
        }
    };

    let file_name = sanitize_filename(field.file_name().unwrap_or("unnamed"));
    tracing::info!("Uploading file: {file_name}");
    let content_type = field
        .content_type()
        .filter(|ct| *ct != "application/octet-stream")
        .map(|s| s.to_string())
        .or_else(|| {
            mime_guess::from_path(&file_name)
                .first()
                .map(|m| m.to_string())
        });

    let id = Uuid::new_v4();
    let delete_token = Uuid::new_v4().to_string().replace('-', "")[..16].to_string();

    tracing::debug!("Starting streaming save for {file_name} (id: {id})");
    let save_result = match state.storage.save_stream(id, field).await {
        Ok(r) => {
            tracing::debug!("Saved {} bytes, sha256: {}", r.size, r.sha256);
            r
        }
        Err(e) => {
            tracing::error!("Storage stream error for {file_name}: {e:?}");
            // Clean up partial file
            let _ = state.storage.delete(id).await;
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Storage error: {e}"),
            );
        }
    };

    let expires_at = if state.default_ttl_hours > 0 {
        Some(Utc::now() + Duration::hours(state.default_ttl_hours as i64))
    } else {
        None
    };

    let meta = FileMetadata {
        id,
        name: file_name.clone(),
        size: save_result.size,
        uploaded_at: Utc::now(),
        share_token: None,
        content_type,
        sha256: Some(save_result.sha256.clone()),
        expires_at,
    };

    if let Err(e) = state.db.insert(&meta, &delete_token) {
        let _ = state.storage.delete(id).await;
        return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}"));
    }

    tracing::info!("Uploaded file: {} ({} bytes)", file_name, save_result.size);

    (
        StatusCode::CREATED,
        Json(UploadResponse {
            id,
            name: meta.name,
            size: meta.size,
            delete_token,
            sha256: save_result.sha256,
        }),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct ListParams {
    page: Option<u32>,
    per_page: Option<u32>,
}

pub async fn list_files(
    State(state): State<AppState>,
    auth: AuthContext,
    Query(params): Query<ListParams>,
) -> Response {
    if let Err(resp) = auth.require_permission("download") {
        return resp;
    }

    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(200);
    match state.db.list(page, per_page) {
        Ok((files, total)) => Json(serde_json::json!({
            "files": files,
            "total": total,
            "page": page,
            "per_page": per_page,
        }))
        .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    }
}

pub async fn get_file(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    match state.db.get(id) {
        Ok(Some(meta)) => {
            if meta.expires_at.is_some_and(|t| t < Utc::now()) {
                return err(StatusCode::GONE, "File has expired");
            }
            Json(meta).into_response()
        }
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

    if meta.expires_at.is_some_and(|t| t < Utc::now()) {
        return err(StatusCode::GONE, "File has expired");
    }

    let (file, total) = match state.storage.open_file(id).await {
        Ok(f) => f,
        Err(e) => {
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Storage error: {e}"),
            );
        }
    };

    let ct = meta
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    serve_range_stream(file, total, &meta.name, ct, &headers).await
}

pub async fn delete_file(
    State(state): State<AppState>,
    auth: OptionalAuthContext,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> Response {
    // Check if user has "delete" or "admin" permission via API token
    let has_api_auth = auth
        .token
        .as_ref()
        .is_some_and(|t| t.permissions.iter().any(|p| p == "delete" || p == "admin"));

    if !has_api_auth {
        // Fall back to per-file delete token from Authorization header
        let provided = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string());

        // Check admin token (legacy)
        let is_admin = state
            .admin_token
            .as_ref()
            .is_some_and(|expected| provided.as_deref() == Some(expected));

        if !is_admin {
            let file_delete_token = match state.db.get_delete_token(id) {
                Ok(Some(t)) => t,
                Ok(None) => return err(StatusCode::NOT_FOUND, "File not found"),
                Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
            };

            let has_delete_token = provided.as_deref().is_some_and(|t| t == file_delete_token);

            if !has_delete_token {
                return err(
                    StatusCode::UNAUTHORIZED,
                    "Invalid or missing token. Use admin token or the file's delete token.",
                );
            }
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

/// HTML share page — shows file info and a download button.
pub async fn share_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Response {
    let meta = match state.db.get_by_share_token(&token) {
        Ok(Some(m)) => m,
        Ok(None) => return err(StatusCode::NOT_FOUND, "Invalid share link"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")),
    };

    if meta.expires_at.is_some_and(|t| t < Utc::now()) {
        return err(StatusCode::GONE, "This share link has expired");
    }

    // If client doesn't want HTML (e.g. curl without Accept header), serve file directly
    let wants_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/html"));

    if !wants_html {
        let (file, total) = match state.storage.open_file(meta.id).await {
            Ok(f) => f,
            Err(e) => {
                return err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Storage error: {e}"),
                );
            }
        };
        let ct = meta
            .content_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        return serve_range_stream(file, total, &meta.name, ct, &headers).await;
    }

    let size_str = humanize_bytes(meta.size);
    let expiry_str = meta
        .expires_at
        .map(|t| format!("Expires: {}", t.format("%Y-%m-%d %H:%M UTC")))
        .unwrap_or_default();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rshare — {name}</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
         background: #0f172a; color: #e2e8f0; display: flex; align-items: center;
         justify-content: center; min-height: 100vh; }}
  .card {{ background: #1e293b; border-radius: 12px; padding: 2.5rem;
           max-width: 420px; width: 90%; text-align: center; box-shadow: 0 4px 24px rgba(0,0,0,0.3); }}
  h1 {{ font-size: 1.1rem; margin-bottom: 1rem; word-break: break-all; }}
  .meta {{ color: #94a3b8; font-size: 0.9rem; margin-bottom: 1.5rem; }}
  .meta span {{ display: block; margin: 0.25rem 0; }}
  a.btn {{ display: inline-block; background: #3b82f6; color: #fff; text-decoration: none;
           padding: 0.75rem 2rem; border-radius: 8px; font-weight: 600; font-size: 1rem;
           transition: background 0.2s; }}
  a.btn:hover {{ background: #2563eb; }}
  .footer {{ margin-top: 1.5rem; color: #475569; font-size: 0.75rem; }}
</style>
</head>
<body>
<div class="card">
  <h1>{name}</h1>
  <div class="meta">
    <span>{size}</span>
    <span>{expiry}</span>
  </div>
  <a class="btn" href="/share/{token}/download">Download</a>
  <p class="footer">Shared via rshare</p>
</div>
</body>
</html>"#,
        name = html_escape(&meta.name),
        size = html_escape(&size_str),
        expiry = html_escape(&expiry_str),
        token = html_escape(&token),
    );

    Html(html).into_response()
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

    if meta.expires_at.is_some_and(|t| t < Utc::now()) {
        return err(StatusCode::GONE, "This share link has expired");
    }

    let (file, total) = match state.storage.open_file(meta.id).await {
        Ok(f) => f,
        Err(e) => {
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Storage error: {e}"),
            );
        }
    };

    let ct = meta
        .content_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    serve_range_stream(file, total, &meta.name, ct, &headers).await
}

/// Serve file data with HTTP Range support via streaming (no full-file buffering).
async fn serve_range_stream(
    mut file: tokio::fs::File,
    total: u64,
    filename: &str,
    content_type: &str,
    headers: &HeaderMap,
) -> Response {
    use tokio::io::AsyncSeekExt;
    use tokio_util::io::ReaderStream;

    // Parse Range header: bytes=START-END (END is optional)
    let (range_start, range_end) = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .map(|v| {
            let mut parts = v.splitn(2, '-');
            let start = parts
                .next()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let end = parts
                .next()
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<u64>().ok())
                .map(|e| e.min(total - 1)); // clamp to file size
            (start, end)
        })
        .unwrap_or((0, None));

    if range_start >= total {
        return Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{total}"))
            .body(Body::empty())
            .unwrap();
    }

    let range_end = range_end.unwrap_or(total - 1);

    if range_start > 0
        && let Err(e) = file.seek(std::io::SeekFrom::Start(range_start)).await
    {
        return err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Seek error: {e}"),
        );
    }

    let length = range_end - range_start + 1;
    let stream = ReaderStream::new(file.take(length));
    let body = Body::from_stream(stream);

    let (status, content_range) = if range_start > 0 || range_end < total - 1 {
        (
            StatusCode::PARTIAL_CONTENT,
            Some(format!("bytes {range_start}-{range_end}/{total}")),
        )
    } else {
        (StatusCode::OK, None)
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_DISPOSITION, content_disposition(filename))
        .header(header::CONTENT_LENGTH, length)
        .header(header::ACCEPT_RANGES, "bytes");

    if let Some(cr) = content_range {
        builder = builder.header("Content-Range", cr);
    }

    builder.body(body).unwrap()
}
