//! Request policy comes from the same live snapshot as admission.
use crate::HostState;
use axum::{extract::State, response::Response};
use gproxy_app::{App, AppData};

pub fn cors_origins<C>(app: &App<C>) -> Vec<String> {
    cors_origins_of(app, &app.data())
}
pub fn trusted_proxies<C>(app: &App<C>) -> Vec<String> {
    trusted_proxies_of(app, &app.data())
}
fn cors_origins_of<C>(app: &App<C>, data: &AppData) -> Vec<String> {
    data.settings
        .as_ref()
        .map(|s| strings(&s.cors_origins))
        .unwrap_or_else(|| app.config().cors_origins.clone())
}
fn trusted_proxies_of<C>(app: &App<C>, data: &AppData) -> Vec<String> {
    data.settings
        .as_ref()
        .map(|s| strings(&s.trusted_proxies))
        .unwrap_or_else(|| app.config().trusted_proxies.clone())
}

/// Both lists as of one snapshot. Every request reads them, from the
/// settings row's JSON; [`HostState::policy_lists`] keeps them parsed for as
/// long as that snapshot is current.
pub(crate) struct PolicyLists {
    pub cors_origins: Vec<String>,
    pub trusted_proxies: Vec<String>,
}

impl PolicyLists {
    pub(crate) fn of<C>(app: &App<C>, data: &AppData) -> Self {
        Self {
            cors_origins: cors_origins_of(app, data),
            trusted_proxies: trusted_proxies_of(app, data),
        }
    }
}
/// The settings row's always-`Secure` switch; off when there is no row yet.
pub fn always_secure_cookie<C>(app: &App<C>) -> bool {
    app.data()
        .settings
        .as_ref()
        .is_some_and(|s| s.always_secure_cookie)
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

/// CORS applies to management routes as well as the inference fallback.
pub async fn cors<C>(
    State(state): State<HostState<C>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response
where
    C: gproxy_seaorm::BatchConnectionTrait + Send + Sync + 'static,
{
    crate::send(async move {
        let lists = state.policy_lists();
        let origin = crate::policy::allowed_origin(request.headers(), &lists.cors_origins);
        if crate::policy::is_preflight(request.method(), request.headers()) {
            use axum::response::IntoResponse;
            return crate::policy::apply_preflight_cors(
                http::StatusCode::NO_CONTENT.into_response(),
                origin.as_ref(),
                request.headers(),
            );
        }
        crate::policy::apply_cors(next.run(request).await, origin.as_ref())
    })
    .await
}
