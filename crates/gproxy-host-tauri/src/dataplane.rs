//! The embedded HTTP host: the data plane, on loopback, in this process.
//!
//! # Why there is an HTTP server inside a desktop application
//!
//! The data plane's clients are other programs — Claude Code, the Codex CLI,
//! anything that speaks an upstream's API — and they speak HTTP. They cannot
//! speak Tauri IPC and never will. So the desktop shell runs
//! [`gproxy_host_axum`] on `127.0.0.1` for them, over the very same [`App`]
//! the IPC commands use. This is the arrangement `design/crates.md` describes
//! and the reason the tauri row in its endpoint table has a cross in the data
//! plane column: the desktop host does not *implement* a data plane, it hosts
//! the one that already exists.
//!
//! # It still demands a key
//!
//! The IPC channel is a trust boundary; a loopback socket is not. Every
//! process on this machine can connect to it, and an unauthenticated data
//! plane would mean every one of them can spend the user's upstream quota. So
//! nothing is relaxed: the same `Authenticator`, the same admission, the same
//! `401`. The key is in the keychain, which is the one place on the machine a
//! passing process cannot read.
//!
//! # And it serves only the data plane
//!
//! [`gproxy_host_axum::router`] mounts `/admin/api` and `/portal/api` as well,
//! because a server needs them: a browser is how an operator reaches a machine
//! they are not sitting at. A desktop shell has a window, so the management
//! surfaces are on IPC and the socket does not need to carry them — and a
//! surface that is not needed is a surface that should not be reachable. The
//! gateway key would otherwise be an administration credential for any local
//! process that got hold of it, rather than a use-this-instance credential.
//!
//! The reduction is a layer in front of the router rather than a different
//! router. The routes, the middleware, the error envelopes and the ingress
//! order stay exactly the host's; this crate only refuses two prefixes, and
//! refuses them with a `404` that says where the surface went.

use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::Request,
    response::{IntoResponse, Response},
};
use gproxy_app::{App, AppConfig};
use gproxy_host_axum::{HostState, router};
use http::StatusCode;
use tokio::net::TcpListener;

use crate::{StartError, StartResult, desktop::Connection};

/// The prefixes the desktop host refuses, and where they went.
const MANAGEMENT_PREFIXES: [&str; 2] = ["/admin/api", "/portal/api"];

/// Bind the data plane and serve it until `stop` is notified.
///
/// Returns the address that was actually bound — which is not the configured
/// one when the configuration asked for port 0, and a test always does.
pub async fn serve(
    app: Arc<App<Connection>>,
    config: &AppConfig,
    stop: Arc<tokio::sync::Notify>,
) -> StartResult<SocketAddr> {
    let ip = config.host.parse::<std::net::IpAddr>().map_err(|_| {
        StartError::App(gproxy_app::AppError::invalid(
            "the listening host must be an IP address",
        ))
    })?;
    let address = SocketAddr::new(ip, config.port);
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| StartError::io(format!("binding {address}"), error))?;
    let bound = listener
        .local_addr()
        .map_err(|error| StartError::io("reading the bound address", error))?;

    // `serve` hands each request its `ConnectInfo`: the client address is how
    // admission counts a rate limit. Without it every peer is unknown — and
    // unknown is deliberately not trusted, so `x-forwarded-for` is ignored and
    // every local process shares one bucket.
    let service = data_plane(HostState::new(app));
    tokio::spawn(async move {
        let result =
            gproxy_host_axum::serve::serve(listener, service, async move { stop.notified().await })
                .await;
        if let Err(error) = result {
            tracing::error!(%error, "the data plane stopped");
        }
    });
    Ok(bound)
}

/// The host's own router with the two management surfaces taken off it.
pub fn data_plane(state: HostState<Connection>) -> axum::Router {
    router(state).layer(axum::middleware::from_fn(refuse_management))
}

/// `404` for `/admin/api` and `/portal/api`, with the reason.
///
/// A `404` rather than a `403`: from outside this process those surfaces do
/// not exist on this host, and saying "forbidden" would invite a client to go
/// looking for a credential that would work.
async fn refuse_management(request: Request, next: axum::middleware::Next) -> Response {
    let path = request.uri().path();
    if MANAGEMENT_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return (
            StatusCode::NOT_FOUND,
            [(http::header::CONTENT_TYPE, "application/json")],
            r#"{"error":{"code":"not_found","message":"the desktop host serves the data plane only; its management surfaces are on Tauri IPC"}}"#,
        )
            .into_response();
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefixes_are_matched_on_a_segment_boundary() {
        // The check `refuse_management` performs, in isolation: a path that
        // merely begins with the same letters is not the surface.
        let refused = |path: &str| {
            MANAGEMENT_PREFIXES
                .iter()
                .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
        };
        assert!(refused("/admin/api"));
        assert!(refused("/admin/api/users"));
        assert!(refused("/portal/api/keys"));
        // A provider whose namespace happens to start that way is data plane.
        assert!(!refused("/admin/apiary/v1/messages"));
        assert!(!refused("/admin"));
        assert!(!refused("/v1/messages"));
        assert!(!refused("/healthz"));
    }
}
