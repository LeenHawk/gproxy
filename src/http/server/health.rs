//! Native framework adapters for liveness and version endpoints.

use axum::response::Response;

/// `GET /healthz`; the router owns the admin gate.
pub async fn healthz() -> Response {
    super::ops_response(crate::http::ops::healthz())
}

/// `GET /version`; the router owns the admin gate.
pub async fn version() -> Response {
    super::ops_response(crate::http::ops::version())
}
