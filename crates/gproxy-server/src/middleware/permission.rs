use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
pub use gproxy_core::{FilePermissionEntry, PermissionEntry};

use crate::app_state::AppState;

/// Axum middleware placeholder for permission checks.
///
/// Permission enforcement is currently done inside the provider handler
/// (after authentication extracts user_id and model resolution is complete).
/// This middleware is a pass-through reserved for future use.
pub async fn permission_middleware(
    State(_state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    next.run(request).await
}

/// Match a model name against a pattern.
pub fn pattern_matches(pattern: &str, model: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return model.starts_with(prefix);
    }
    pattern == model
}
