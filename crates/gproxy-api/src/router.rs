use std::sync::Arc;

use axum::Router;
use axum::middleware::from_fn_with_state;
use axum::routing::post;
use tower_http::limit::RequestBodyLimitLayer;

use gproxy_server::AppState;

use crate::auth::{require_admin_middleware, require_user_session_middleware};
use crate::cors::CorsLayer;
use crate::downstream_log::downstream_log_middleware;

const MAX_REQUEST_BODY_BYTES: usize = 50 * 1024 * 1024;

/// Build the complete API router.
pub fn api_router(state: Arc<AppState>) -> Router {
    let admin_router =
        crate::admin::router().layer(from_fn_with_state(state.clone(), require_admin_middleware));
    let user_router = crate::user::router().layer(from_fn_with_state(
        state.clone(),
        require_user_session_middleware,
    ));
    let app_router = Router::new()
        .route("/login", post(crate::login::login))
        .nest("/admin", admin_router)
        .nest("/user", user_router)
        .layer(RequestBodyLimitLayer::new(MAX_REQUEST_BODY_BYTES));

    Router::new()
        .merge(app_router)
        .merge(crate::provider::router(state.clone()))
        .layer(from_fn_with_state(state.clone(), downstream_log_middleware))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
