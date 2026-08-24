use axum::body::to_bytes;
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::{CONNECTION, UPGRADE};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use gproxy_core::{RequestCtx, RoutingMode};

use crate::response::HostResponse;
use crate::server::{HostState, MAX_BODY_BYTES};

pub(crate) async fn handle(State(state): State<HostState>, request: Request) -> Response {
    let request_id = state.request_id();
    let (mut parts, body) = request.into_parts();
    let method = parts.method.clone();
    let path = parts.uri.path().to_owned();
    let query = parts.uri.query().map(str::to_owned);
    let headers = parts.headers.clone();
    let websocket = match websocket_upgrade(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(response) => return response,
    };
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
    let (mode, path) = normalize_path(&path);
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

fn normalize_path(path: &str) -> (RoutingMode, String) {
    if is_api_path(path) {
        return (RoutingMode::Aggregated, path.to_owned());
    }
    let Some((name, remainder)) = path.strip_prefix('/').and_then(|path| path.split_once('/'))
    else {
        return (RoutingMode::Aggregated, path.to_owned());
    };
    let remainder = format!("/{remainder}");
    if name.is_empty() || !is_api_path(&remainder) {
        return (RoutingMode::Aggregated, path.to_owned());
    }
    (
        RoutingMode::Named {
            name: name.to_owned(),
        },
        remainder,
    )
}

fn is_api_path(path: &str) -> bool {
    path == "/v1" || path.starts_with("/v1/") || path == "/v1beta" || path.starts_with("/v1beta/")
}
