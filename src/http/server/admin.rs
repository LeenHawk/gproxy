//! Thin native axum adapter for the shared admin/portal dispatcher.

use std::net::IpAddr;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, Method, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::any;

use crate::app::AppState;
use crate::http::admin_api::{Request as AdminRequest, Resp};

pub fn admin_router(state: AppState) -> Router<AppState> {
    let mut router = Router::new()
        .route("/admin/{*rest}", any(dispatch))
        .route("/user/{*rest}", any(dispatch));
    if !state.config.cors_origins.is_empty() {
        router = router.layer(crate::http::cors::credentialed_admin_layer(
            &state.config.cors_origins,
        ));
    }
    router
}

async fn dispatch(
    State(state): State<AppState>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    connect: Option<axum::Extension<ConnectInfo<std::net::SocketAddr>>>,
    body: Bytes,
) -> Response {
    let source_ip = client_ip(
        &headers,
        connect.map(|value| value.0.0.ip()),
        &state.config.trusted_proxies,
    );
    let request = AdminRequest::new(method, uri, headers).with_source_ip(source_ip);
    match crate::http::admin_api::dispatch(&state, &request, &body).await {
        Some(response) => into_axum(response),
        None => crate::api::error::ApiError::NotFound("not found".into()).into_response(),
    }
}

fn into_axum(response: Resp) -> Response {
    let mut out = Response::new(Body::from(response.body));
    *out.status_mut() = response.status;
    *out.headers_mut() = response.headers;
    out
}

fn client_ip(
    headers: &HeaderMap,
    peer: Option<IpAddr>,
    trusted_proxies: &[IpAddr],
) -> Option<String> {
    let is_trusted = |ip: &IpAddr| ip.is_loopback() || trusted_proxies.contains(ip);
    if let Some(peer) = peer
        && !is_trusted(&peer)
    {
        return Some(peer.to_string());
    }
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find(|value| {
            value
                .parse::<IpAddr>()
                .map(|ip| !is_trusted(&ip))
                .unwrap_or(true)
        })
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
        .or_else(|| peer.map(|value| value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_peer_ignores_forwarding_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "6.6.6.6".parse().unwrap());
        let peer = Some("203.0.113.9".parse().unwrap());
        assert_eq!(
            client_ip(&headers, peer, &[]).as_deref(),
            Some("203.0.113.9")
        );
    }

    #[test]
    fn trusted_peer_walks_xff_from_the_right() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.1.1.1, 198.51.100.7, 10.0.0.2".parse().unwrap(),
        );
        let trusted = ["10.0.0.2".parse().unwrap()];
        let peer = Some("127.0.0.1".parse().unwrap());
        assert_eq!(
            client_ip(&headers, peer, &trusted).as_deref(),
            Some("198.51.100.7")
        );
    }
}
