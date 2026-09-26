//! The console in the window, answered by the server's own router.
//!
//! # Why one command and not the table
//!
//! The console is one application for both hosts, and every call it makes goes
//! through a single `fetch` of an `/admin/api` or `/portal/api` path. Those
//! surfaces carry more than the operations [`crate::ipc::table`] binds: the
//! caller's [`AdminScope`](gproxy_app::AdminScope) and the section gate, the
//! scope-narrowed `credentials` and `quotas`, `/context`, the audit action
//! derived from the matched route, the refresh after a write. Translating each
//! path into a command would re-derive all of that on this side, and it would
//! drift the first time a route changed shape.
//!
//! So the window's request is handed, in process, to
//! [`gproxy_host_axum::router`] — the same routes, middleware and error
//! envelopes a server answers with — and the response goes back over IPC. No
//! socket is involved: the embedded data plane still refuses both surfaces
//! (see [`crate::dataplane`]), so nothing else on the machine can reach them.
//!
//! # Who the request is
//!
//! The IPC channel is the trust boundary, as it is for every command here. The
//! router still authenticates, so the request carries the gateway key — a key
//! of the local administrator — as its bearer token, and whatever cookie or
//! `Authorization` the page tried to send is dropped. The page chooses the
//! path, the method, the body and the admin scope; it cannot choose who it is.

use std::net::{Ipv4Addr, SocketAddr};

use axum::{body::Body, extract::ConnectInfo};
use http::{HeaderName, HeaderValue, Method, Request, header};
use serde::{Deserialize, Serialize};
use tower::ServiceExt;

use crate::{Desktop, IpcError, IpcResult};

/// The two surfaces the console calls. Anything else — the data plane, the
/// health probe — is for other programs and has a socket of its own.
const SURFACES: [&str; 2] = ["/admin/api/", "/portal/api/"];

/// The request headers that mean something to the router. Everything else the
/// page sends is dropped, and `authorization` and `cookie` above all.
const FORWARDED_HEADERS: [&str; 3] = ["accept", "content-type", gproxy_app::SCOPE_HEADER];

/// One `fetch` from the console, as it crosses the bridge.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleRequest {
    pub method: String,
    /// The path and query, `/admin/api/providers?limit=50`.
    pub path: String,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
}

/// The router's answer. The body is text because every console route answers
/// JSON or nothing; a `204` is an empty string.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
}

/// Run one console request through the management router.
pub async fn request(desktop: &Desktop, request: ConsoleRequest) -> IpcResult<ConsoleResponse> {
    let route = request.path.split('?').next().unwrap_or_default();
    if !SURFACES.iter().any(|surface| route.starts_with(surface)) {
        return Err(refused(format!("`{route}` is not a console surface")));
    }
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| refused(format!("`{}` is not an HTTP method", request.method)))?;

    let mut builder = Request::builder().method(method).uri(&request.path);
    for (name, value) in &request.headers {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        if FORWARDED_HEADERS.contains(&name.as_str())
            && let Ok(value) = HeaderValue::from_str(value)
        {
            builder = builder.header(name, value);
        }
    }
    let bearer = HeaderValue::from_str(&format!("Bearer {}", desktop.data_plane().gateway_key))
        .map_err(IpcError::internal)?;
    let mut http_request = builder
        .header(header::AUTHORIZATION, bearer)
        .body(Body::from(request.body.unwrap_or_default()))
        .map_err(|error| refused(format!("building the request: {error}")))?;
    // Admission counts rate limits per peer. The window is this machine, and
    // saying so is what an accepted loopback connection would have said.
    http_request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))));

    let response = desktop
        .management()
        .clone()
        .oneshot(http_request)
        .await
        .map_err(IpcError::internal)?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .map_err(IpcError::internal)?;
    Ok(ConsoleResponse {
        status,
        content_type,
        body: String::from_utf8_lossy(&body).into_owned(),
    })
}

fn refused(message: String) -> IpcError {
    gproxy_app::AppError::invalid(message).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_two_console_surfaces_are_bridged() {
        let bridged = |path: &str| SURFACES.iter().any(|surface| path.starts_with(surface));
        assert!(bridged("/admin/api/providers"));
        assert!(bridged("/portal/api/context"));
        assert!(!bridged("/v1/messages"));
        assert!(!bridged("/healthz"));
        assert!(!bridged("/admin/apiary/v1/messages"));
    }
}
