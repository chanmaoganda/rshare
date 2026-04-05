use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Json, Response};
use rshare_common::{ApiToken, ErrorResponse};

use crate::AppState;

/// Auth context extracted from the request. If auth is not enabled (no tokens in DB),
/// `token` will be `None` and requests pass through freely.
/// **Rejects** the request if tokens are configured but none matches.
pub struct AuthContext {
    pub token: Option<ApiToken>,
}

/// Like `AuthContext` but never rejects. Returns `None` if the token is invalid
/// or missing. Use this for endpoints that have fallback auth (e.g., delete with
/// per-file tokens).
pub struct OptionalAuthContext {
    pub token: Option<ApiToken>,
}

impl AuthContext {
    pub fn require_permission(&self, permission: &str) -> Result<(), Response> {
        match &self.token {
            Some(t)
                if t.permissions
                    .iter()
                    .any(|p| p == permission || p == "admin") =>
            {
                Ok(())
            }
            Some(_) => Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: format!("Missing required permission: {permission}"),
                }),
            )
                .into_response()),
            // None means auth is not enabled (no tokens configured)
            None => Ok(()),
        }
    }
}

/// Try to resolve a Bearer token from headers against the DB and legacy admin token.
fn resolve_token(parts: &Parts, state: &AppState) -> Option<ApiToken> {
    let raw = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))?;

    // Check legacy admin_token
    if let Some(admin) = &state.admin_token
        && raw == admin
    {
        return Some(ApiToken {
            name: "admin".to_string(),
            permissions: vec!["admin".to_string()],
            created_at: chrono::Utc::now(),
        });
    }

    // Check DB tokens
    state.db.get_token_by_hash(raw).ok().flatten()
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // If no tokens exist in DB, auth is disabled (backward compat)
        if !state.db.has_any_tokens().unwrap_or(false) {
            return Ok(AuthContext { token: None });
        }

        if let Some(token) = resolve_token(parts, state) {
            return Ok(AuthContext { token: Some(token) });
        }

        // Token required but not provided or invalid
        Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Valid API token required".to_string(),
            }),
        )
            .into_response())
    }
}

impl FromRequestParts<AppState> for OptionalAuthContext {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if !state.db.has_any_tokens().unwrap_or(false) {
            return Ok(OptionalAuthContext { token: None });
        }
        Ok(OptionalAuthContext {
            token: resolve_token(parts, state),
        })
    }
}
