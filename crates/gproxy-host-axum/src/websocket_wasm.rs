//! What a websocket handshake means on a fetch runtime: not yet.
//!
//! The native module this stands in for is built on hyper's `OnUpgrade` and
//! `axum::extract::ws`, and neither exists inside a Worker. Cloudflare upgrades
//! a request by constructing a `WebSocketPair` and returning the client half in
//! a `Response`'s `webSocket` field — a mechanism with no HTTP shape at all, so
//! it cannot be reached through `http::Response` and cannot be a drop-in for
//! the native path.
//!
//! This module exists so [`crate::ingress`] does not have to know that. It
//! keeps the same three entry points with the same signatures, and every one of
//! them answers `501`: a realtime client is told plainly that this deployment
//! cannot serve it, rather than being handed a `101` nothing will ever write to.
//!
//! What it would take to make this real is written down in
//! `crates/gproxy-host-edge/README.md`: a `WebSocketPair` implementation of
//! [`gproxy_protocol::connection::WebSocket`] and a way for a handler to hand
//! the prepared JS `Response` back to the fetch entry point — the request's
//! extensions can carry one, which is the seam to use. It is deliberately not
//! done here: an upgrade path that has never been run against a real client is
//! worth less than an honest refusal.

use std::sync::Arc;

use axum::response::{IntoResponse, Response};
use gproxy_app::{App, Caller, DataPlaneRequest, ServiceRequestIn};
use gproxy_seaorm::BatchConnectionTrait;
use http::{StatusCode, header};

use crate::response::CancelOnDrop;

/// The extracted handshake. There is never one on this target, so the type is
/// uninhabitable — which is what makes the two branches in [`crate::ingress`]
/// that consume it dead code rather than wrong code.
#[derive(Debug)]
pub enum Upgrade {}

/// Always a refusal, and always before anything is accepted — same position in
/// the order as the native one, so a caller that authenticated learns why and
/// a caller that did not learns nothing.
pub async fn extract(
    _parts: &mut http::request::Parts,
    _max_frame_bytes: u64,
) -> Result<Upgrade, Box<Response>> {
    Err(Box::new(unavailable()))
}

pub async fn data_plane<C>(
    _app: Arc<App<C>>,
    upgrade: Upgrade,
    _caller: &Caller,
    _request: DataPlaneRequest,
    _max_frame_bytes: u64,
    _cancel: CancelOnDrop,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    match upgrade {}
}

pub async fn service<C>(
    _app: &Arc<App<C>>,
    upgrade: Upgrade,
    _caller: &Caller,
    _request: ServiceRequestIn,
    _max_frame_bytes: u64,
    _capture: Option<gproxy_app::capture::DownstreamCapture>,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    match upgrade {}
}

fn unavailable() -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"error":{"code":"unsupported","message":"this host cannot serve websocket sessions; realtime needs a deployment with socket upgrades"}}"#,
    )
        .into_response()
}
