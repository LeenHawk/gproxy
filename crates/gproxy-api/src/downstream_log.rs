use std::sync::Arc;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use bytes::Bytes;
use futures_util::StreamExt;

use gproxy_server::AppState;

use crate::auth::extract_api_key;
use crate::provider::handler::generate_trace_id;

/// Trace ID extension set by the downstream log middleware so handlers
/// can reference the same trace for upstream logs and usage records.
#[derive(Debug, Clone, Copy)]
pub struct TraceId(pub i64);

/// Unified downstream request logging middleware.
///
/// Captures request and response metadata for ALL routes (admin, user,
/// login, provider). For streaming responses (text/event-stream) it
/// wraps the body to accumulate chunks and log them at stream end.
/// WebSocket upgrades (101) are logged without a response body.
pub async fn downstream_log_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let config = state.config();
    if !config.enable_downstream_log {
        drop(config);
        let trace_id = generate_trace_id();
        request.extensions_mut().insert(TraceId(trace_id));
        return next.run(request).await;
    }
    let include_body = config.enable_downstream_log_body;
    drop(config);

    let trace_id = generate_trace_id();
    request.extensions_mut().insert(TraceId(trace_id));

    // Resolve user from token in headers (session or API key).
    let (user_id, user_key_id) = resolve_user(&state, request.headers());

    // Capture request metadata.
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(String::from);
    let req_headers = headers_to_json(request.headers());

    // Buffer request body so both the middleware and handler can read it.
    let (parts, body) = request.into_parts();
    let req_bytes = axum::body::to_bytes(body, 50 * 1024 * 1024)
        .await
        .map(|b| b.to_vec())
        .unwrap_or_default();
    let req_body_for_log = if include_body {
        Some(req_bytes.clone())
    } else {
        None
    };
    let request = Request::from_parts(parts, Body::from(req_bytes));

    let response = next.run(request).await;

    let status = response.status().as_u16() as i32;
    let resp_headers = headers_to_json(response.headers());

    let is_streaming = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"));
    let is_ws = response.status() == http::StatusCode::SWITCHING_PROTOCOLS;

    if is_ws {
        record(
            &state,
            trace_id,
            user_id,
            user_key_id,
            method,
            path,
            query,
            req_headers,
            req_body_for_log,
            status,
            resp_headers,
            None,
        )
        .await;
        return response;
    }

    if is_streaming {
        let (parts, body) = response.into_parts();
        let state2 = state.clone();
        let wrapped = async_stream::stream! {
            let mut accumulated: Vec<u8> = Vec::new();
            let mut body_stream = body.into_data_stream();
            while let Some(chunk) = body_stream.next().await {
                match chunk {
                    Ok(data) => {
                        if include_body {
                            accumulated.extend_from_slice(&data);
                        }
                        yield Ok::<Bytes, axum::Error>(data);
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
            let body_for_log = if accumulated.is_empty() { None } else { Some(accumulated) };
            record(
                &state2, trace_id, user_id, user_key_id, method, path, query,
                req_headers, req_body_for_log, status, resp_headers, body_for_log,
            )
            .await;
        };
        return Response::from_parts(parts, Body::from_stream(wrapped));
    }

    // Normal response: buffer body and log.
    let (parts, body) = response.into_parts();
    let resp_bytes = axum::body::to_bytes(body, 50 * 1024 * 1024)
        .await
        .map(|b| b.to_vec())
        .unwrap_or_default();
    let resp_body_for_log = if include_body {
        Some(resp_bytes.clone())
    } else {
        None
    };

    record(
        &state,
        trace_id,
        user_id,
        user_key_id,
        method,
        path,
        query,
        req_headers,
        req_body_for_log,
        status,
        resp_headers,
        resp_body_for_log,
    )
    .await;

    Response::from_parts(parts, Body::from(resp_bytes))
}

/// Try to resolve user identity from the Authorization header.
fn resolve_user(state: &AppState, headers: &http::HeaderMap) -> (Option<i64>, Option<i64>) {
    let token = match extract_api_key(headers) {
        Ok(t) => t,
        Err(_) => return (None, None),
    };

    if token.starts_with("sess-") {
        if let Some(session) = state.validate_session(&token) {
            return (Some(session.user_id), None);
        }
        return (None, None);
    }

    if let Some(user_key) = state.authenticate_api_key(&token) {
        return (Some(user_key.user_id), Some(user_key.id));
    }

    (None, None)
}

#[allow(clippy::too_many_arguments)]
async fn record(
    state: &AppState,
    trace_id: i64,
    user_id: Option<i64>,
    user_key_id: Option<i64>,
    method: String,
    path: String,
    query: Option<String>,
    req_headers: String,
    req_body: Option<Vec<u8>>,
    status: i32,
    resp_headers: String,
    resp_body: Option<Vec<u8>>,
) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertDownstreamRequest(
            gproxy_storage::DownstreamRequestWrite {
                trace_id,
                at_unix_ms: now_ms,
                internal: false,
                user_id,
                user_key_id,
                request_method: method,
                request_headers_json: req_headers,
                request_path: path,
                request_query: query,
                request_body: req_body,
                response_status: Some(status),
                response_headers_json: resp_headers,
                response_body: resp_body,
            },
        ))
        .await;
}

fn headers_to_json(headers: &http::HeaderMap) -> String {
    let map: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or("")))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "[]".to_string())
}
