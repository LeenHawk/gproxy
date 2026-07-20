//! The generic request orchestrator (§6.3). Sequences the already-separated
//! steps for both routing modes; stream & non-stream share every step and
//! diverge only at the body tail inside [`failover`](crate::pipeline::failover).

#[cfg(target_arch = "wasm32")]
use bytes::Bytes;
use tracing::Instrument;

use crate::app::AppState;
use crate::billing::pending;
use crate::pipeline::candidate;
use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::pipeline::{auth, capture, classify, failover, ingress, model_catalog};
use crate::protocol::Operation;

/// Drive one request to an [`ExecOutcome`], wrapped in a per-request tracing
/// span (§15.2) carrying `request_id` and — recorded as they resolve —
/// `op` / `kind` / `route` / `provider`.
pub async fn execute(state: &AppState, ctx: RequestCtx) -> Result<ExecOutcome, PipelineError> {
    let span = tracing::info_span!(
        "request",
        request_id = %ctx.request_id,
        op = tracing::field::Empty,
        kind = tracing::field::Empty,
        route = tracing::field::Empty,
        provider = tracing::field::Empty,
    );
    // §8-D downstream capture: snapshot the inbound wire facts BEFORE run()
    // (the ingress blacklist strips client creds in place); the row is written
    // below once the final status is known. None when the toggle is off.
    let downstream = capture::downstream_precapture(state, &ctx);
    let result = run(state, ctx).instrument(span).await;
    if let Some(cap) = downstream {
        // §8-D response body (fold-in for non-streaming; streamed bodies are
        // backfilled by `settle` since they aren't materialized here).
        let want_body = state.cp().log_settings.enable_downstream_log_body;
        let (status, resp_body): (http::StatusCode, Option<bytes::Bytes>) = match &result {
            Ok(o) => {
                let b = match &o.body {
                    ResponseBody::Stream(_) => None,
                    ResponseBody::Full(b) if want_body => Some(b.clone()),
                    ResponseBody::Full(_) => None,
                };
                (o.status, b)
            }
            Err(e) => (
                e.status(),
                want_body.then(|| bytes::Bytes::from(e.error_body_json())),
            ),
        };
        capture::log_downstream(state, cap, status, resp_body.as_deref()).await;
    }
    result
}

/// Inner orchestrator (§6.3). Sequences the already-separated steps for both
/// routing modes; stream & non-stream share every step and diverge only at the
/// body tail inside [`failover`](crate::pipeline::failover).
async fn run(state: &AppState, mut ctx: RequestCtx) -> Result<ExecOutcome, PipelineError> {
    let span = tracing::Span::current();
    // One synchronous snapshot scope covers auth and every control-plane lookup
    // needed by this request. The returned plans own their data; no ArcSwap
    // guard reaches cache, persistence, or network I/O below.
    let prepared = {
        let cp = state.cp();
        ctx.identity = Some(auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref())?);
        ingress::apply_global_blacklist(&mut ctx);
        ingress::normalize_multipart_form_body(&mut ctx)?;
        let classified = classify::classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body)?;
        ctx.op = Some(classified.op);
        ctx.stream = classified.stream;
        span.record("op", tracing::field::debug(classified.op.operation));
        span.record("kind", tracing::field::debug(classified.op.kind));

        if matches!(ctx.mode, RoutingMode::Aggregated)
            && matches!(
                classified.op.operation,
                Operation::ListModels | Operation::GetModel
            )
        {
            None
        } else {
            Some(candidate::prepare(&cp, &ctx, classified.op)?)
        }
    };

    let Some(prepared) = prepared else {
        return model_catalog::serve_aggregated(state, &ctx).await;
    };
    let request = match prepared {
        candidate::Prepared::ScopedModels(models) => {
            span.record("provider", models.provider_name());
            return models.serve(state).await;
        }
        candidate::Prepared::Candidates(request) => *request,
    };
    if let Some(route) = request.route_name() {
        span.record("route", route);
        ctx.route_name = Some(route.to_owned());
    }
    if let Some(provider) = request.provider_name() {
        span.record("provider", provider);
    }
    let admitted = request
        .admit(
            state,
            ctx.identity.as_deref().expect("auth ran first"),
            ctx.stream,
        )
        .await?;
    let candidate::Admitted {
        candidates,
        est_micros,
        quota_scopes,
        synthesize_stream,
    } = admitted;

    // §17 pre-deduct: admission passed — charge the estimate to every
    // quota-bearing scope now, before any upstream byte. Settle refunds
    // the exact amount; the error path below refunds when nothing settles.
    let pending_micros = if quota_scopes.is_empty() {
        0
    } else {
        est_micros
    };
    pending::charge(state.cache.as_ref(), &quota_scopes, pending_micros).await;
    ctx.pending_micros = pending_micros;

    #[cfg(not(target_arch = "wasm32"))]
    if synthesize_stream {
        return Ok(crate::pipeline::stream::synthetic_outcome(
            state.clone(),
            ctx,
            candidates,
            quota_scopes,
            pending_micros,
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    let result = failover::run_failover(state, &ctx, &candidates).await;
    #[cfg(target_arch = "wasm32")]
    let mut result = failover::run_failover(state, &ctx, &candidates).await;
    // Only a 2xx content response attaches a SettleCtx (whose settle refunds
    // the pending). Everything else — pipeline error, all-candidates-failed,
    // or a relayed permanent 4xx — must refund here. A crash in between
    // self-heals via the 15-minute pending TTL.
    if !matches!(&result, Ok(o) if o.status.is_success()) {
        pending::refund(state.cache.as_ref(), &quota_scopes, pending_micros).await;
    }

    // Edge pass-through streams are live. A synthetic stream still waits for
    // its deliberately non-streaming upstream, then encodes that one buffered
    // response into the requested stream wire shape.
    #[cfg(target_arch = "wasm32")]
    if synthesize_stream
        && let Ok(outcome) = &mut result
        && outcome.status.is_success()
        && let ResponseBody::Full(body) = &outcome.body
    {
        let kind = match ctx.op.expect("classified").kind {
            crate::protocol::OperationKind::ContentGeneration(kind) => kind,
            crate::protocol::OperationKind::Provider(_) => unreachable!("synthetic content"),
        };
        let gemini_json = kind == crate::protocol::ContentGenerationKind::GeminiGenerateContent
            && !ctx
                .query
                .as_deref()
                .is_some_and(|query| query.split('&').any(|part| part == "alt=sse"));
        let encoded = if gemini_json {
            let value: serde_json::Value = serde_json::from_slice(body).map_err(|error| {
                PipelineError::TransformResponse(crate::transform::TransformError::InvalidInput {
                    reason: format!("synthetic gemini response is not JSON: {error}"),
                })
            })?;
            Bytes::from(serde_json::to_vec(&vec![value]).map_err(|error| {
                PipelineError::TransformResponse(crate::transform::TransformError::Serialization {
                    reason: error.to_string(),
                })
            })?)
        } else {
            Bytes::from(
                crate::transform::stream_adapter::synthesize_sse(kind, body)
                    .map_err(PipelineError::TransformResponse)?,
            )
        };
        outcome.body = ResponseBody::Full(encoded);
        outcome.headers.remove(http::header::CONTENT_LENGTH);
        outcome.headers.insert(
            http::header::CONTENT_TYPE,
            if gemini_json {
                http::HeaderValue::from_static("application/json")
            } else {
                http::HeaderValue::from_static("text/event-stream")
            },
        );
    }
    result
}
