//! Liveness and version endpoints.

use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    pub status: &'static str,
}

/// `GET /healthz` — liveness probe. Sits behind the same admin gate as
/// `/admin/*` (the `require_admin` middleware owns the auth); probes must
/// send an admin session cookie or an admin user's API key.
pub async fn healthz() -> Json<Health> {
    Json(Health { status: "ok" })
}

#[derive(Serialize)]
pub struct Version {
    pub version: &'static str,
}

/// `GET /version` — report the running binary version. Admin-gated like
/// [`healthz`] (a bare version string still fingerprints the deployment).
pub async fn version() -> Json<Version> {
    Json(Version {
        version: env!("CARGO_PKG_VERSION"),
    })
}
