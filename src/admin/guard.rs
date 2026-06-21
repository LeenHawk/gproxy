//! Cross-target session guards — used by both the native admin middleware and
//! the edge dispatcher. Each guard runs the CSRF check first, then authenticates.

use crate::admin::session::{AdminUser, SessionUser};
use crate::api::error::ApiError;
use crate::app::AppState;

/// Require an admin caller for a request described by its parts.
///
/// 1. Runs [`crate::admin::csrf::csrf_ok`] on the method + headers. A cross-origin
///    cookie-authenticated mutation is rejected with 403 Forbidden.
/// 2. Calls [`crate::admin::authenticate_admin`] — session cookie or admin API key.
///    Returns 401 Unauthorized if neither is valid.
pub async fn guard_admin(
    state: &AppState,
    parts: &http::request::Parts,
) -> Result<AdminUser, ApiError> {
    if !crate::admin::csrf::csrf_ok(&parts.method, &parts.headers, &state.config.cors_origins) {
        return Err(ApiError::Forbidden(
            "cross-origin admin request refused".into(),
        ));
    }
    crate::admin::authenticate_admin(state, &parts.headers)
        .await
        .ok_or(ApiError::Unauthorized)
}

/// Require any enabled user session for a request described by its parts.
///
/// 1. Runs [`crate::admin::csrf::csrf_ok`] on the method + headers. A cross-origin
///    cookie-authenticated mutation is rejected with 403 Forbidden.
/// 2. Calls [`crate::admin::authenticate_session`] — session cookie only.
///    Returns 401 Unauthorized if the cookie is missing or invalid.
pub async fn guard_session(
    state: &AppState,
    parts: &http::request::Parts,
) -> Result<SessionUser, ApiError> {
    if !crate::admin::csrf::csrf_ok(&parts.method, &parts.headers, &state.config.cors_origins) {
        return Err(ApiError::Forbidden("cross-origin request refused".into()));
    }
    crate::admin::authenticate_session(state, &parts.headers)
        .await
        .ok_or(ApiError::Unauthorized)
}
