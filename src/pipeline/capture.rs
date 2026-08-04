//! §8-D request capture: `downstream_requests` / `upstream_requests` wire logs
//! gated by the instance log toggles (§8-E), with §14.3 secret redaction.
//! Ordinary native writes are fire-and-forget spawns; rows requiring a later
//! streaming-body backfill are inserted inline first. wasm awaits inline (no
//! detached tasks on edge).

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::app::AppState;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::store::persistence::records::{DownstreamRequestInput, UpstreamRequestInput};
use crate::util::time::unix_now;
#[cfg(not(target_arch = "wasm32"))]
use tracing::Instrument as _;

mod client;

use crate::http::redaction::{body_string, headers_json, redact_query, warn_unless_redacted};
pub use client::CapturingClient;

/// The downstream wire facts, captured BEFORE the pipeline mutates the request
/// (the ingress blacklist strips client creds in place); written after the
/// response status is known. `None` = downstream capture disabled.
pub struct DownstreamCapture {
    at: i64,
    request_id: String,
    method: String,
    path: String,
    query: Option<String>,
    headers_json: Option<Value>,
    body: Option<String>,
}

/// Capture the inbound request if `enable_downstream_log` is on.
pub fn downstream_precapture(state: &AppState, ctx: &RequestCtx) -> Option<DownstreamCapture> {
    let ls = state.cp().log_settings.clone();
    if !ls.enable_downstream_log {
        return None;
    }
    let redact = warn_unless_redacted(&ls);
    Some(DownstreamCapture {
        at: unix_now(),
        request_id: ctx.request_id.clone(),
        method: ctx.method.to_string(),
        path: ctx.path.clone(),
        query: ctx.query.as_deref().map(|q| redact_query(q, redact)),
        headers_json: Some(headers_json(&ctx.headers, redact)),
        body: ls
            .enable_downstream_log_body
            .then(|| body_string(&ctx.body, redact)),
    })
}

/// Append the captured downstream request with its final `status`. For
/// non-streaming responses the body is folded into the same INSERT via
/// `response_body` (streaming responses pass `None` and backfill later via
/// [`record_downstream_response`]). Gated by the downstream log-body toggle.
pub async fn log_downstream(
    state: &AppState,
    cap: DownstreamCapture,
    status: StatusCode,
    response_body: Option<&[u8]>,
) {
    let resp = response_body.map(|b| {
        let ls = state.cp().log_settings.clone();
        let redact = warn_unless_redacted(&ls);
        body_string(b, redact)
    });
    let input = DownstreamRequestInput {
        request_id: cap.request_id,
        at: cap.at,
        method: cap.method,
        path: cap.path,
        query: cap.query,
        status: i64::from(status.as_u16()),
        headers_json: cap.headers_json,
        body: cap.body,
        response_body: resp,
    };
    persist(state, Row::Downstream(input)).await;
}

/// The final attempt's wire facts handed to [`log_upstream`].
pub struct UpstreamWire<'a> {
    pub status: StatusCode,
    pub latency_ms: i64,
    pub url: &'a str,
    pub method: &'a http::Method,
    /// Prepared request headers — captured by the attempt only when the
    /// upstream-log toggle was on.
    pub sent_headers: Option<&'a HeaderMap>,
    pub sent_body: &'a Bytes,
    /// Buffered upstream response body; `None` for streams (a guard backfills
    /// the exact row after consuming bytes at that path's capture seam).
    pub resp_body: Option<&'a Bytes>,
}

/// Stable correlation for one captured upstream transport call. The request id
/// prevents a stale guard from targeting a reused database row id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpstreamCaptureId {
    row_id: i64,
    request_id: String,
    provider_id: i64,
}

/// Append the final (returned-to-client) upstream attempt's wire facts if
/// `enable_upstream_log` is on.
pub async fn log_upstream(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    w: UpstreamWire<'_>,
) {
    let Some(input) = upstream_input(
        state,
        &ctx.request_id,
        cand.provider.id,
        cand.credential.id,
        w,
    ) else {
        return;
    };
    persist(state, Row::Upstream(input)).await;
}

/// Insert an upstream row inline and return its primary key. Streaming capture
/// uses this before exposing the body so its later backfill cannot race the
/// INSERT or select another call sharing the downstream request id.
async fn insert_upstream_raw(
    state: &AppState,
    request_id: &str,
    provider_id: i64,
    credential_id: i64,
    w: UpstreamWire<'_>,
) -> Option<UpstreamCaptureId> {
    let input = upstream_input(state, request_id, provider_id, credential_id, w)?;
    match crate::store::persistence::PersistenceBackend::append_upstream_request(
        state.persistence.as_ref(),
        input,
    )
    .await
    {
        Ok(row) => Some(UpstreamCaptureId {
            row_id: row.id,
            request_id: row.request_id,
            provider_id,
        }),
        Err(e) => {
            tracing::warn!(
                request_id,
                row_type = "upstream",
                provider_id,
                credential_id,
                error = %e,
                "upstream request-capture write failed"
            );
            None
        }
    }
}

/// Start a direct streaming capture before the stream is returned. Custom
/// exchanges do this inside [`CapturingClient`] for the exact transport call.
pub(crate) async fn start_upstream_stream(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    w: UpstreamWire<'_>,
) -> Option<UpstreamCaptureId> {
    insert_upstream_raw(
        state,
        &ctx.request_id,
        cand.provider.id,
        cand.credential.id,
        w,
    )
    .await
}

fn upstream_input(
    state: &AppState,
    request_id: &str,
    provider_id: i64,
    credential_id: i64,
    w: UpstreamWire<'_>,
) -> Option<UpstreamRequestInput> {
    let ls = state.cp().log_settings.clone();
    if !ls.enable_upstream_log {
        return None;
    }
    let redact = warn_unless_redacted(&ls);
    Some(UpstreamRequestInput {
        request_id: request_id.to_owned(),
        at: unix_now(),
        provider_id: Some(provider_id),
        credential_id: Some(credential_id),
        url: w.url.to_owned(),
        method: w.method.to_string(),
        status: i64::from(w.status.as_u16()),
        latency_ms: w.latency_ms,
        headers_json: w.sent_headers.map(|h| headers_json(h, redact)),
        body: ls
            .enable_upstream_log_body
            .then(|| body_string(w.sent_body, redact)),
        response_body: w
            .resp_body
            .filter(|_| ls.enable_upstream_log_body)
            .map(|b| body_string(b, redact)),
    })
}

enum Row {
    Downstream(DownstreamRequestInput),
    Upstream(UpstreamRequestInput),
}

async fn persist(state: &AppState, row: Row) {
    async fn write(db: &dyn crate::store::persistence::PersistenceBackend, row: Row) {
        match row {
            Row::Downstream(input) => {
                let request_id = input.request_id.clone();
                if let Err(error) =
                    crate::store::persistence::PersistenceBackend::append_downstream_request(
                        db, input,
                    )
                    .await
                {
                    tracing::warn!(
                        request_id,
                        row_type = "downstream",
                        error = %error,
                        "downstream request-capture write failed"
                    );
                }
            }
            Row::Upstream(input) => {
                let request_id = input.request_id.clone();
                let provider_id = input.provider_id;
                let credential_id = input.credential_id;
                if let Err(error) =
                    crate::store::persistence::PersistenceBackend::append_upstream_request(
                        db, input,
                    )
                    .await
                {
                    tracing::warn!(
                        request_id,
                        row_type = "upstream",
                        ?provider_id,
                        ?credential_id,
                        error = %error,
                        "upstream request-capture write failed"
                    );
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let persistence = std::sync::Arc::clone(&state.persistence);
        let span = tracing::Span::current();
        tokio::spawn(async move { write(persistence.as_ref(), row).await }.instrument(span));
    }
    #[cfg(target_arch = "wasm32")]
    write(state.persistence.as_ref(), row).await;
}

/// Backfill the captured DOWNSTREAM response body for a streaming response (the
/// row was appended before the stream settled). Gated by the downstream
/// log-body toggle; redacted + capped by `body_string`. The caller chooses
/// whether to await or detach this write.
pub async fn record_downstream_response(state: &AppState, request_id: &str, body: &[u8]) {
    let ls = state.cp().log_settings.clone();
    if !(ls.enable_downstream_log && ls.enable_downstream_log_body) {
        return;
    }
    let redact = warn_unless_redacted(&ls);
    let s = body_string(body, redact);
    persist_response(state, RespRow::Downstream(request_id.to_owned(), s)).await;
}

/// Backfill the captured UPSTREAM response body for a streaming response.
pub(crate) async fn record_upstream_response(
    state: &AppState,
    capture_id: UpstreamCaptureId,
    body: &[u8],
) {
    let ls = state.cp().log_settings.clone();
    if !(ls.enable_upstream_log && ls.enable_upstream_log_body) {
        return;
    }
    let redact = warn_unless_redacted(&ls);
    let s = body_string(body, redact);
    persist_response(state, RespRow::Upstream(capture_id, s)).await;
}

enum RespRow {
    Downstream(String, String),
    Upstream(UpstreamCaptureId, String),
}

async fn persist_response(state: &AppState, row: RespRow) {
    async fn write(db: &dyn crate::store::persistence::PersistenceBackend, row: RespRow) {
        match row {
            RespRow::Downstream(rid, body) => {
                if let Err(error) =
                    crate::store::persistence::PersistenceBackend::update_downstream_response(
                        db,
                        &rid,
                        Some(body),
                    )
                    .await
                {
                    tracing::warn!(
                        request_id = rid,
                        row_type = "downstream",
                        error = %error,
                        "downstream response-capture write failed"
                    );
                }
            }
            RespRow::Upstream(capture_id, body) => {
                let UpstreamCaptureId {
                    row_id,
                    request_id,
                    provider_id,
                } = capture_id;
                if let Err(error) =
                    crate::store::persistence::PersistenceBackend::update_upstream_response_by_id(
                        db,
                        row_id,
                        &request_id,
                        Some(body),
                    )
                    .await
                {
                    tracing::warn!(
                        request_id,
                        row_type = "upstream",
                        provider_id,
                        error = %error,
                        "upstream response-capture write failed"
                    );
                }
            }
        }
    }
    write(state.persistence.as_ref(), row).await;
}
