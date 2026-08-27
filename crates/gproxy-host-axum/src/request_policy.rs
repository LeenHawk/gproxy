use axum::http::{HeaderMap, HeaderValue, Method};
use axum::response::Response;

use crate::server::HostState;

pub(crate) fn client_ip(
    peer: std::net::IpAddr,
    headers: &HeaderMap,
    trusted: &[std::net::IpAddr],
) -> std::net::IpAddr {
    if !peer.is_loopback() && !trusted.contains(&peer) {
        return peer;
    }
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok())
        })
        .unwrap_or(peer)
}

pub(crate) fn is_upload(request: &gproxy_core::RequestCtx) -> bool {
    request.method == Method::POST
        && matches!(request.path.as_str(), "/v1/files" | "/upload/v1beta/files")
}

pub(crate) fn allowed_origin(state: &HostState, origin: &HeaderValue) -> bool {
    origin
        .to_str()
        .is_ok_and(|origin| state.cors_origins.iter().any(|allowed| allowed == origin))
}

pub(crate) fn apply_cors(mut response: Response, origin: Option<&HeaderValue>) -> Response {
    let Some(origin) = origin else {
        return response;
    };
    let headers = response.headers_mut();
    headers.insert("access-control-allow-origin", origin.clone());
    headers.insert(
        "access-control-allow-credentials",
        HeaderValue::from_static("true"),
    );
    headers.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, POST, PATCH, DELETE, OPTIONS"),
    );
    headers.insert(
        "access-control-allow-headers",
        HeaderValue::from_static("authorization, content-type, x-api-key"),
    );
    headers.append(http::header::VARY, HeaderValue::from_static("Origin"));
    response
}
