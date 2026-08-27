use axum::body::to_bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{ConnectInfo, FromRequestParts, Request, State};
use axum::http::header::{CONNECTION, UPGRADE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use gproxy_core::RequestCtx;

use crate::response::HostResponse;
use crate::server::{HostState, MAX_BODY_BYTES};

pub(crate) async fn handle(
    State(state): State<HostState>,
    ConnectInfo(peer): ConnectInfo<std::net::SocketAddr>,
    request: Request,
) -> Response {
    let origin = request
        .headers()
        .get(http::header::ORIGIN)
        .cloned()
        .filter(|origin| crate::request_policy::allowed_origin(&state, origin));
    if request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key("access-control-request-method")
    {
        let response = StatusCode::NO_CONTENT.into_response();
        return crate::request_policy::apply_cors(response, origin.as_ref());
    }
    let response = handle_request(state, peer, request).await;
    crate::request_policy::apply_cors(response, origin.as_ref())
}

async fn handle_request(
    state: HostState,
    peer: std::net::SocketAddr,
    request: Request,
) -> Response {
    let request_id = state.request_id();
    tracing::debug!(
        request_id,
        instance_name = %state.app.instance_name(),
        "request accepted"
    );
    let (mut parts, body) = request.into_parts();
    parts.extensions.insert(gproxy_admin::AuthSource(
        crate::request_policy::client_ip(peer.ip(), &parts.headers, &state.trusted_proxies)
            .to_string(),
    ));
    let method = parts.method.clone();
    let path = parts.uri.path().to_owned();
    let query = parts.uri.query().map(str::to_owned);
    let headers = parts.headers.clone();
    let permit = state
        .semaphore
        .clone()
        .acquire_owned()
        .await
        .expect("host semaphore remains open");
    let body = match to_bytes(body, MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    if (path == "/admin" || path.starts_with("/admin/"))
        && let Some(response) = state.app.admin_dispatch(&parts, body.clone()).await
    {
        return crate::response::buffered_response(response, permit, &request_id);
    }
    if (path == "/portal/api" || path.starts_with("/portal/api/"))
        && let Some(response) = state.app.portal_dispatch(&parts, body.clone()).await
    {
        return crate::response::buffered_response(response, permit, &request_id);
    }
    if let Some(response) = crate::static_assets::serve(&parts) {
        return crate::response::buffered_response(response, permit, &request_id);
    }
    let websocket = match websocket_upgrade(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(response) => return response,
    };
    let (mode, path) = gproxy_app::ingress::normalize_path(&path);
    let request = RequestCtx {
        request_id: request_id.clone(),
        method,
        path,
        query,
        headers,
        body,
        upgrade: websocket.is_some(),
        mode,
    };
    let _upload = if crate::request_policy::is_upload(&request) {
        state
            .uploads
            .acquire(state.app.file_upload_max_in_flight())
            .await
    } else {
        None
    };
    let result = state.app.execute(request).await;
    HostResponse::new(result, websocket, permit, request_id).into_response()
}

async fn websocket_upgrade(
    parts: &mut Parts,
    state: &HostState,
) -> Result<Option<WebSocketUpgrade>, Response> {
    if !has_websocket_intent(&parts.headers) {
        return Ok(None);
    }
    WebSocketUpgrade::from_request_parts(parts, state)
        .await
        .map(Some)
        .map_err(IntoResponse::into_response)
}

fn has_websocket_intent(headers: &HeaderMap) -> bool {
    headers
        .keys()
        .any(|name| name.as_str().starts_with("sec-websocket-"))
        || headers
            .get_all(UPGRADE)
            .iter()
            .any(|value| contains_token(value, b"websocket"))
        || headers
            .get_all(CONNECTION)
            .iter()
            .any(|value| contains_token(value, b"upgrade"))
}

fn contains_token(value: &HeaderValue, expected: &[u8]) -> bool {
    value
        .as_bytes()
        .split(|byte| *byte == b',')
        .map(trim_ascii)
        .any(|token| token.eq_ignore_ascii_case(expected))
}

fn trim_ascii(mut value: &[u8]) -> &[u8] {
    while value.first().is_some_and(u8::is_ascii_whitespace) {
        value = &value[1..];
    }
    while value.last().is_some_and(u8::is_ascii_whitespace) {
        value = &value[..value.len() - 1];
    }
    value
}
