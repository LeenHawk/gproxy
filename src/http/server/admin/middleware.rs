//! Admin auth middleware: session cookie OR an admin user's API key, else 401.
//! State-changing cookie-authenticated requests also pass a same-origin (CSRF)
//! check before auth.

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api::error::ApiError;
use crate::app::AppState;

/// Gate a router behind [`authenticate_admin`](crate::admin::authenticate_admin)
/// (admin session cookie or an admin user's API key). On success the resolved
/// [`AdminUser`](crate::admin::session::AdminUser) is inserted into request
/// extensions for handlers.
///
/// A same-origin (CSRF) check runs first for state-changing methods — see
/// [`crate::admin::csrf::csrf_ok`]. It only constrains cookie-authenticated
/// browser requests; header (API-key) automation is untouched.
pub async fn require_admin(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if !crate::admin::csrf::csrf_ok(req.method(), req.headers(), &state.config.cors_origins) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "cross-origin admin request refused",
        )
            .into_response();
    }
    match crate::admin::authenticate_admin(&state, req.headers()).await {
        Some(admin) => {
            req.extensions_mut().insert(admin);
            next.run(req).await
        }
        None => ApiError::Unauthorized.into_response(),
    }
}

/// Gate a router behind [`authenticate_session`](crate::admin::authenticate_session)
/// (session cookie, any enabled user). On success the resolved
/// [`SessionUser`](crate::admin::session::SessionUser) is inserted into request
/// extensions for handlers.
///
/// Reuses the same [`crate::admin::csrf::csrf_ok`] check as [`require_admin`]:
/// state-changing cookie-authenticated requests must pass the same-origin guard.
pub async fn require_session(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if !crate::admin::csrf::csrf_ok(req.method(), req.headers(), &state.config.cors_origins) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            "cross-origin request refused",
        )
            .into_response();
    }
    match crate::admin::authenticate_session(&state, req.headers()).await {
        Some(user) => {
            req.extensions_mut().insert(user);
            next.run(req).await
        }
        None => ApiError::Unauthorized.into_response(),
    }
}
