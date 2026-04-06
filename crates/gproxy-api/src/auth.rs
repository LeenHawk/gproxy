use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use subtle::ConstantTimeEq;

use gproxy_server::AppState;
use gproxy_server::principal::MemoryUserKey;

use crate::error::HttpError;

#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub MemoryUserKey);

/// Extract API key from request headers.
/// Checks: Authorization: Bearer <key>, x-api-key, x-goog-api-key.
pub fn extract_api_key(headers: &HeaderMap) -> Result<String, HttpError> {
    // Authorization: Bearer <key>
    if let Some(value) = headers.get("authorization")
        && let Ok(s) = value.to_str()
        && let Some(token) = s
            .strip_prefix("Bearer ")
            .or_else(|| s.strip_prefix("bearer "))
    {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // x-api-key
    if let Some(value) = headers.get("x-api-key")
        && let Ok(s) = value.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    // x-goog-api-key
    if let Some(value) = headers.get("x-goog-api-key")
        && let Ok(s) = value.to_str()
    {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    Err(HttpError::unauthorized("missing API key"))
}

/// Authenticate a user API key and return the key record.
///
/// Rejects the admin key to prevent identity ambiguity — the admin key
/// must only authenticate via `authorize_admin`, never as a user.
pub fn authenticate_user(
    headers: &HeaderMap,
    state: &AppState,
) -> Result<MemoryUserKey, HttpError> {
    let api_key = extract_api_key(headers)?;
    // Reject admin key in user auth path to prevent identity ambiguity
    let config = state.config();
    if api_key.as_bytes().ct_eq(config.admin_key.as_bytes()).into() {
        return Err(HttpError::forbidden(
            "admin key cannot be used for user authentication",
        ));
    }
    state
        .authenticate_api_key(&api_key)
        .ok_or_else(|| HttpError::unauthorized("invalid or disabled API key"))
}

/// Authenticate as admin (check against global admin_key).
pub fn authorize_admin(headers: &HeaderMap, state: &AppState) -> Result<(), HttpError> {
    let api_key = extract_api_key(headers)?;
    let config = state.config();
    if api_key.as_bytes().ct_eq(config.admin_key.as_bytes()).into() {
        Ok(())
    } else {
        Err(HttpError::forbidden("admin access required"))
    }
}

pub async fn require_user_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    match authenticate_user(request.headers(), &state) {
        Ok(user_key) => {
            request.extensions_mut().insert(AuthenticatedUser(user_key));
            next.run(request).await
        }
        Err(err) => err.into_response(),
    }
}

pub async fn require_admin_middleware(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    match authorize_admin(request.headers(), &state) {
        Ok(()) => next.run(request).await,
        Err(err) => err.into_response(),
    }
}

/// Authenticated session user (from session token, not API key).
#[derive(Debug, Clone)]
pub struct SessionUser {
    pub user_id: i64,
    pub user_key_id: i64,
}

/// Middleware for /user/* routes: requires a session token (from /login).
///
/// Session tokens are short-lived (24h) and memory-only.
/// This separates user management auth from provider proxy auth,
/// so a leaked inference API key cannot be used to generate new keys
/// or enumerate existing ones.
pub async fn require_user_session_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let token = match extract_api_key(request.headers()) {
        Ok(t) => t,
        Err(err) => return err.into_response(),
    };

    // Accept session tokens (sess-*) for /user/* routes
    if token.starts_with("sess-") {
        match state.validate_session(&token) {
            Some((user_id, user_key_id)) => {
                request.extensions_mut().insert(SessionUser { user_id, user_key_id });
                return next.run(request).await;
            }
            None => {
                return HttpError::unauthorized("session expired or invalid").into_response();
            }
        }
    }

    // Fallback: also accept admin key for /user/* (admin can do anything)
    let config = state.config();
    if token.as_bytes().ct_eq(config.admin_key.as_bytes()).into() {
        // Admin accessing user routes — use user_id=0 sentinel
        request.extensions_mut().insert(SessionUser { user_id: 0, user_key_id: 0 });
        return next.run(request).await;
    }

    HttpError::unauthorized("session token required (use /login to obtain one)").into_response()
}
