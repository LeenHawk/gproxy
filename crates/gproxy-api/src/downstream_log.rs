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
///
/// The log entry is committed through a `LogGuard` whose `Drop` impl
/// spawns the DB write. This guarantees a record lands in the request
/// log in three otherwise-silent failure modes:
///   1. A panic anywhere after the guard is installed (unwind runs Drop).
///   2. A streaming response cancelled by a client disconnect (hyper
///      drops the generator future, which drops the captured guard).
///   3. A streaming response that errored mid-stream (same as above).
///
/// In all three cases the record is written with whatever partial state
/// the guard had at the moment it was dropped, and `status` stays `None`
/// if the response line was never observed.
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

    // Install the log guard. Every path from here on flushes through Drop,
    // so panics and cancelled stream futures still produce a log entry.
    let mut guard = LogGuard::new(
        state.clone(),
        DownstreamRecord {
            trace_id,
            user_id,
            user_key_id,
            method,
            path,
            query,
            req_headers,
            req_body: req_body_for_log,
            status: None,
            resp_headers: "[]".to_string(),
            resp_body: None,
        },
    );

    let response = next.run(request).await;

    let status = response.status().as_u16() as i32;
    let resp_headers = headers_to_json(response.headers());
    guard.set_response_meta(status, resp_headers);

    let is_streaming = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"));
    let is_ws = response.status() == http::StatusCode::SWITCHING_PROTOCOLS;

    if is_ws {
        // Guard commits on drop at function return.
        return response;
    }

    if is_streaming {
        let (parts, body) = response.into_parts();
        let wrapped = async_stream::stream! {
            // Move guard into the generator so its Drop fires whether the
            // stream completes, errors out, or is cancelled by a disconnect.
            let mut guard = guard;
            let mut body_stream = body.into_data_stream();
            while let Some(chunk) = body_stream.next().await {
                match chunk {
                    Ok(data) => {
                        if include_body {
                            guard.append_resp_body(&data);
                        }
                        yield Ok::<Bytes, axum::Error>(data);
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
            drop(guard);
        };
        return Response::from_parts(parts, Body::from_stream(wrapped));
    }

    // Normal response: buffer body and log.
    let (parts, body) = response.into_parts();
    let resp_bytes = axum::body::to_bytes(body, 50 * 1024 * 1024)
        .await
        .map(|b| b.to_vec())
        .unwrap_or_default();
    if include_body {
        guard.set_resp_body(resp_bytes.clone());
    }

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

struct DownstreamRecord {
    trace_id: i64,
    user_id: Option<i64>,
    user_key_id: Option<i64>,
    method: String,
    path: String,
    query: Option<String>,
    req_headers: String,
    req_body: Option<Vec<u8>>,
    status: Option<i32>,
    resp_headers: String,
    resp_body: Option<Vec<u8>>,
}

/// RAII guard that guarantees the request log entry is flushed.
///
/// Normal path: the middleware fills `status`/`resp_headers`/`resp_body`
/// and the guard falls out of scope at function return, firing `Drop`.
/// Streaming path: the guard is moved into the `async_stream` generator
/// and fires `Drop` when the generator is dropped (normal completion,
/// stream error, or hyper cancelling the body on client disconnect).
/// Panic path: unwinding runs `Drop` and the log entry is still written.
///
/// `Drop` spawns a detached task to perform the async DB write, so the
/// guard works from a synchronous `drop` context.
struct LogGuard {
    state: Option<Arc<AppState>>,
    record: Option<DownstreamRecord>,
}

impl LogGuard {
    fn new(state: Arc<AppState>, record: DownstreamRecord) -> Self {
        Self {
            state: Some(state),
            record: Some(record),
        }
    }

    fn set_response_meta(&mut self, status: i32, headers: String) {
        if let Some(r) = self.record.as_mut() {
            r.status = Some(status);
            r.resp_headers = headers;
        }
    }

    fn set_resp_body(&mut self, body: Vec<u8>) {
        if let Some(r) = self.record.as_mut() {
            r.resp_body = Some(body);
        }
    }

    fn append_resp_body(&mut self, data: &[u8]) {
        if let Some(r) = self.record.as_mut() {
            r.resp_body
                .get_or_insert_with(Vec::new)
                .extend_from_slice(data);
        }
    }
}

impl Drop for LogGuard {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };
        let Some(record) = self.record.take() else {
            return;
        };
        // Drop may run from any sync context including a panic unwind. Use
        // `try_current` so a missing runtime (unit tests, shutdown) becomes
        // a silent no-op instead of its own panic.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                write_record(&state, record).await;
            });
        }
    }
}

async fn write_record(state: &AppState, r: DownstreamRecord) {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertDownstreamRequest(
            gproxy_storage::DownstreamRequestWrite {
                trace_id: r.trace_id,
                at_unix_ms: now_ms,
                internal: false,
                user_id: r.user_id,
                user_key_id: r.user_key_id,
                request_method: r.method,
                request_headers_json: r.req_headers,
                request_path: r.path,
                request_query: r.query,
                request_body: r.req_body,
                response_status: r.status,
                response_headers_json: r.resp_headers,
                response_body: r.resp_body,
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
