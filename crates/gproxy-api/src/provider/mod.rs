use std::sync::Arc;

use axum::Router;
use axum::middleware::{from_fn, from_fn_with_state};
use axum::routing::{get, post};
use tower_http::limit::RequestBodyLimitLayer;

use gproxy_server::AppState;
use gproxy_server::middleware::classify::classify_middleware;
use gproxy_server::middleware::model_alias::model_alias_middleware;
use gproxy_server::middleware::request_model::request_model_middleware;
use gproxy_server::middleware::sanitize::sanitize_middleware;

use crate::auth::{require_admin_middleware, require_user_middleware};

pub mod handler;
pub mod oauth;
pub mod websocket;
pub mod ws_bridge;

const MAX_PROVIDER_REQUEST_BODY_BYTES: usize = 50 * 1024 * 1024;
const MAX_PROVIDER_FILE_REQUEST_BODY_BYTES: usize = 500 * 1024 * 1024;

fn with_proxy_http_layers(
    router: Router<Arc<AppState>>,
    state: Arc<AppState>,
    max_body_bytes: usize,
) -> Router<Arc<AppState>> {
    router
        .layer(from_fn(sanitize_middleware))
        .layer(from_fn_with_state(state.clone(), model_alias_middleware))
        .layer(from_fn(request_model_middleware))
        .layer(from_fn(classify_middleware))
        .layer(from_fn_with_state(state, require_user_middleware))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
}

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    let proxy_http_non_file_router = Router::new()
        // Scoped routes: /{provider}/v1/...
        .route("/{provider}/v1/messages", post(handler::proxy))
        .route("/{provider}/v1/messages/count-tokens", post(handler::proxy))
        .route("/{provider}/v1/chat/completions", post(handler::proxy))
        .route("/{provider}/v1/responses", post(handler::proxy))
        .route(
            "/{provider}/v1/responses/input_tokens",
            post(handler::proxy),
        )
        .route("/{provider}/v1/responses/compact", post(handler::proxy))
        .route("/{provider}/v1/embeddings", post(handler::proxy))
        .route("/{provider}/v1/images/generations", post(handler::proxy))
        .route("/{provider}/v1/images/edits", post(handler::proxy))
        .route("/{provider}/v1/models", get(handler::proxy))
        .route("/{provider}/v1/models/{*model_id}", get(handler::proxy))
        .route("/{provider}/v1beta/models", get(handler::proxy))
        // `/{provider}/v1beta/models/{*target}` must exist as an explicit POST route
        // (not just under the catch-all below) because the WebSocket router registers
        // the same path with GET. Without this, matchit picks the WS route for any
        // `POST /.../v1beta/models/*:generateContent` and replies 405 Method Not
        // Allowed with an empty body. Router::merge combines the GET (ws) + POST
        // (http) method handlers when the path patterns match exactly.
        .route("/{provider}/v1beta/models/{*target}", post(handler::proxy))
        .route("/{provider}/v1beta/{*target}", post(handler::proxy))
        // Unscoped routes (provider determined by model prefix or alias)
        .route("/v1/messages", post(handler::proxy_unscoped))
        .route("/v1/messages/count_tokens", post(handler::proxy_unscoped))
        .route("/v1/chat/completions", post(handler::proxy_unscoped))
        .route("/v1/responses", post(handler::proxy_unscoped))
        .route("/v1/responses/input_tokens", post(handler::proxy_unscoped))
        .route("/v1/responses/compact", post(handler::proxy_unscoped))
        .route("/v1/embeddings", post(handler::proxy_unscoped))
        .route("/v1/images/generations", post(handler::proxy_unscoped))
        .route("/v1/images/edits", post(handler::proxy_unscoped))
        .route("/v1/models", get(handler::proxy_unscoped))
        .route("/v1/models/{*model_id}", get(handler::proxy_unscoped))
        // Unscoped Gemini v1beta routes (model in path carries provider prefix)
        .route("/v1beta/models", get(handler::proxy_unscoped))
        .route("/v1beta/{*target}", post(handler::proxy_unscoped));

    let proxy_http_file_router = Router::new()
        .route(
            "/{provider}/v1/files",
            post(handler::proxy).get(handler::proxy),
        )
        .route(
            "/{provider}/v1/files/{file_id}",
            get(handler::proxy).delete(handler::proxy),
        )
        .route(
            "/{provider}/v1/files/{file_id}/content",
            get(handler::proxy),
        )
        // Unscoped file operations (provider from X-Provider header)
        .route(
            "/v1/files",
            post(handler::proxy_unscoped_files).get(handler::proxy_unscoped_files),
        )
        .route(
            "/v1/files/{file_id}",
            get(handler::proxy_unscoped_files).delete(handler::proxy_unscoped_files),
        )
        .route(
            "/v1/files/{file_id}/content",
            get(handler::proxy_unscoped_files),
        );

    let proxy_http_router = with_proxy_http_layers(
        proxy_http_non_file_router,
        state.clone(),
        MAX_PROVIDER_REQUEST_BODY_BYTES,
    )
    .merge(with_proxy_http_layers(
        proxy_http_file_router,
        state.clone(),
        MAX_PROVIDER_FILE_REQUEST_BODY_BYTES,
    ));

    let proxy_ws_router = Router::new()
        .route(
            "/{provider}/v1/responses",
            get(websocket::openai_responses_ws),
        )
        .route(
            "/{provider}/v1beta/models/{*target}",
            get(websocket::gemini_live),
        )
        .route(
            "/v1/responses",
            get(websocket::openai_responses_ws_unscoped),
        )
        .layer(from_fn(sanitize_middleware))
        .layer(from_fn_with_state(state.clone(), require_user_middleware));

    let provider_admin_router = Router::new()
        .route("/{provider}/v1/oauth", get(oauth::oauth_start))
        .route("/{provider}/v1/oauth/callback", get(oauth::oauth_callback))
        .route("/{provider}/v1/usage", get(oauth::upstream_usage))
        .layer(from_fn_with_state(state, require_admin_middleware));

    proxy_http_router
        .merge(proxy_ws_router)
        .merge(provider_admin_router)
}
