use axum::Router;
use axum::routing::{get, post};
use gproxy_server::AppState;
use std::sync::Arc;

pub mod keys;
pub mod quota;
pub mod usages;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/keys/query", post(keys::query_keys))
        .route("/keys/generate", post(keys::generate_key))
        .route("/keys/update-enabled", post(keys::update_key_enabled))
        .route("/keys/delete", post(keys::delete_key))
        .route("/quota", get(quota::get_quota))
        .route("/usages/query", post(usages::query_usages))
        .route("/usages/count", post(usages::count_usages))
        .route("/usages/summary", post(usages::summarize_usages))
}
