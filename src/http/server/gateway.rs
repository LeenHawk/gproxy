//! Gateway handlers: read the inbound request, run the pipeline, relay the
//! upstream response. Aggregated (`/v1/...`) and scoped (`/{provider}/v1/...`).

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use crate::app::AppState;
use crate::channel::http_util::sanitize_response_headers;
use crate::config::MAX_BODY_BYTES;
use crate::http::server::extract::build_ctx;
use crate::pipeline;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};

/// `/v1/{*rest}` — model name resolves to a route.
pub async fn aggregated(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, false).await
}

/// `/{provider}/v1/{*rest}` — bypass routing, hit the named provider directly.
pub async fn scoped(State(state): State<AppState>, req: Request) -> Response {
    handle(state, req, true).await
}

async fn handle(state: AppState, req: Request, scoped: bool) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    let ctx = match build_ctx(parts, bytes, scoped) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let request_id = ctx.request_id.clone();
    match pipeline::execute(&state, ctx).await {
        Ok(outcome) => egress(outcome, &request_id),
        Err(e) => e.into_response(),
    }
}

/// Map an [`ExecOutcome`] to the client response: status + hop-by-hop-sanitized
/// headers + the buffered or (native) streamed body, plus the request id for
/// correlation.
fn egress(outcome: ExecOutcome, request_id: &str) -> Response {
    let mut builder = Response::builder().status(outcome.status);
    if let Some(h) = builder.headers_mut() {
        *h = sanitize_response_headers(&outcome.headers);
        if let Ok(v) = HeaderValue::from_str(request_id) {
            h.insert(HeaderName::from_static("x-gproxy-request-id"), v);
        }
    }
    let body = match outcome.body {
        ResponseBody::Full(b) => Body::from(b),
        #[cfg(not(target_arch = "wasm32"))]
        ResponseBody::Stream(s) => Body::from_stream(s),
    };
    builder
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
