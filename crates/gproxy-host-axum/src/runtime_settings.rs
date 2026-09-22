//! Request policy comes from the same live snapshot as admission.
use crate::HostState;
use axum::{extract::State, response::Response};
use gproxy_app::App;

pub fn cors_origins<C>(app: &App<C>) -> Vec<String> {
    app.data()
        .settings
        .as_ref()
        .map(|s| strings(&s.cors_origins))
        .unwrap_or_else(|| app.config().cors_origins.clone())
}
pub fn trusted_proxies<C>(app: &App<C>) -> Vec<String> {
    app.data()
        .settings
        .as_ref()
        .map(|s| strings(&s.trusted_proxies))
        .unwrap_or_else(|| app.config().trusted_proxies.clone())
}
fn strings(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect()
}

pub async fn info<C>(State(state): State<HostState<C>>) -> Response {
    let data = state.app().data();
    crate::error::ok_json(&serde_json::json!({
        "instanceName": data.settings.as_ref().map(|s| s.instance_name.as_str()).unwrap_or("GPROXY"),
        "version": env!("CARGO_PKG_VERSION"),
        "hash": env!("GPROXY_BUILD_HASH"),
    }))
}
