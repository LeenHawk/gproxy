//! Response body materialization for a completed upstream attempt.

use std::sync::Arc;

use bytes::Bytes;
use http::StatusCode;
use serde_json::Value;

use super::BodySource;
use crate::app::AppState;
use crate::channel::{Channel, ShapeCtx};
use crate::pipeline::context::RequestCtx;
use crate::pipeline::error::PipelineError;
use crate::pipeline::outcome::ResponseBody;
use crate::pipeline::settle;
use crate::pipeline::transform::{self as transform_step, TransformPlan};
use crate::protocol::ContentGenerationKind;

/// Output of [`materialize`]: the client-facing body plus, for non-streaming
/// responses, the captured upstream (provider) response body (§8-D). Streaming
/// upstream capture is handled inline by the spliced `capture_raw_stream` guard,
/// so `upstream_raw` is `None` for streams.
pub(in crate::pipeline::failover) struct Materialized {
    pub body: ResponseBody,
    pub upstream_raw: Option<Bytes>,
    pub settle: Option<BufferedSettle>,
}

pub(in crate::pipeline::failover) struct BufferedSettle {
    pub ctx: settle::SettleCtx,
    pub body: Bytes,
    pub stream: bool,
}

/// What [`materialize`] needs to capture a streaming upstream response body.
/// `Some` only when upstream response-body logging is enabled.
pub(in crate::pipeline::failover) struct UpstreamRespCapture {
    pub state: AppState,
    pub request_id: String,
}

pub(in crate::pipeline::failover) struct ResponseRuleCtx<'a> {
    pub rules: &'a [crate::process::CompiledRule],
    pub model: &'a str,
}

/// Materialize an attempt's body. Response-direction transform applies only to
/// 2xx bodies — error payloads stay provider-native (M2 fidelity note). When
/// `upstream` is `Some`, the post-decode provider response is captured for
/// §8-D logging (buffered: returned via `upstream_raw`; streaming: backfilled by
/// the spliced guard).
#[allow(clippy::too_many_arguments)]
pub(in crate::pipeline::failover) async fn materialize(
    channel: &Arc<dyn Channel>,
    source: BodySource,
    plan: &TransformPlan,
    ctx: &RequestCtx,
    status: StatusCode,
    provider_settings: &Value,
    response_rules: Option<ResponseRuleCtx<'_>>,
    upstream: Option<UpstreamRespCapture>,
    settle_ctx: Option<settle::SettleCtx>,
) -> Result<Materialized, PipelineError> {
    match source {
        BodySource::Buffered(b) => {
            let shape = ShapeCtx {
                op: plan.shape_op(ctx),
                stream: plan.upstream_stream(ctx),
                status,
                settings: provider_settings,
            };
            // shape_response runs on ALL statuses (error bodies included).
            let b = channel.shape_response(b, &shape);
            let kind = match shape.op.kind {
                crate::protocol::OperationKind::ContentGeneration(k) => Some(k),
                crate::protocol::OperationKind::Provider(_) => None,
            };
            let b = match (status.is_success(), response_rules.as_ref()) {
                (true, Some(response_rules)) => crate::process::apply_response(
                    response_rules.rules,
                    shape.op,
                    kind,
                    response_rules.model,
                    b,
                ),
                _ => b,
            };
            // §8-D: capture the post-decode provider response. For the buffered
            // aggregate path (codex/kiro non-stream) the real decode happens in
            // `materialize_buffered` → decode here too so the log matches the
            // streaming arm + the "post-decode" contract, not raw binary frames.
            let upstream_raw = upstream.as_ref().map(|_| {
                if status.is_success() && plan.is_aggregate_stream() && !ctx.stream {
                    Bytes::from(decode_buffered_stream(channel, &b))
                } else {
                    b.clone()
                }
            });
            let settle_stream = plan.settle_stream(ctx);
            let settle = settle_ctx.map(|settle_ctx| BufferedSettle {
                ctx: settle_ctx,
                body: b.clone(),
                stream: settle_stream,
            });
            let body = materialize_buffered(channel, plan, ctx, status, b)?;
            Ok(Materialized {
                body,
                upstream_raw,
                settle,
            })
        }
        BodySource::Streaming(st) => {
            if !status.is_success() {
                // Streamed error: undecoded passthrough, no upstream capture.
                return Ok(Materialized {
                    body: ResponseBody::Stream(crate::pipeline::stream::into_byte_stream(st)),
                    upstream_raw: None,
                    settle: None,
                });
            }
            // Order: raw upstream → channel decoder (envelope/binary → canonical
            // provider SSE) → [§8-D raw capture tee] → M2 transform (provider →
            // inbound, or identity on passthrough) → client.
            let st = match channel.stream_decoder() {
                Some(dec) => crate::pipeline::stream::channel_decode_stream(st, dec),
                None => crate::pipeline::stream::into_byte_stream(st),
            };
            let shape_op = plan.shape_op(ctx);
            let kind = match shape_op.kind {
                crate::protocol::OperationKind::ContentGeneration(k) => Some(k),
                crate::protocol::OperationKind::Provider(_) => None,
            };
            let st = match response_rules.as_ref().and_then(|response_rules| {
                crate::process::response_stream_decoder(
                    response_rules.rules,
                    shape_op,
                    kind,
                    response_rules.model,
                )
            }) {
                Some(dec) => crate::pipeline::stream::channel_decode_stream(st, dec),
                None => st,
            };
            let st = match settle_ctx {
                Some(ctx) => crate::pipeline::stream::instrument_settle_stream(
                    st,
                    settle::StreamGuard::new(ctx),
                ),
                None => st,
            };
            // Tee the post-decode (provider-native) bytes for upstream logging
            // BEFORE any cross-protocol transform.
            let st = match upstream {
                Some(cap) => crate::pipeline::stream::capture_raw_stream(
                    st,
                    crate::pipeline::stream::RawCaptureGuard::new(cap.state, cap.request_id),
                ),
                None => st,
            };
            if status.is_success() && plan.is_aggregate_stream() && !ctx.stream {
                let b = collect_byte_stream(st).await?;
                let agg = aggregate_buffered_stream(channel, plan.target_kind(), &b);
                return Ok(Materialized {
                    body: ResponseBody::Full(transform_step::aggregate_response_body(plan, agg)?),
                    upstream_raw: None,
                    settle: None,
                });
            }
            let body = match transform_step::stream_transformer(plan) {
                None => ResponseBody::Stream(st),
                Some(t) => {
                    ResponseBody::Stream(crate::pipeline::stream::transform_byte_stream(st, t))
                }
            };
            Ok(Materialized {
                body,
                upstream_raw: None,
                settle: None,
            })
        }
    }
}

async fn collect_byte_stream(
    st: crate::pipeline::outcome::ByteStream,
) -> Result<Bytes, PipelineError> {
    use futures_util::TryStreamExt;

    let chunks: Vec<Bytes> = st
        .try_collect()
        .await
        .map_err(|error| PipelineError::Transport(error.to_string()))?;
    Ok(Bytes::from(chunks.concat()))
}

/// The buffered-body conversion ladder, split out so [`materialize`] stays
/// focused on capture + stream wiring.
fn materialize_buffered(
    channel: &Arc<dyn Channel>,
    plan: &TransformPlan,
    ctx: &RequestCtx,
    status: StatusCode,
    b: Bytes,
) -> Result<ResponseBody, PipelineError> {
    if status.is_success() && plan.is_synthesize_stream() {
        return Ok(ResponseBody::Full(transform_step::response_body(plan, b)?));
    }
    // Non-stream client over a force-streamed upstream (codex/kiro): collapse
    // the buffered event-stream into one object, then convert the target wire
    // back to the inbound wire.
    if status.is_success() && plan.is_aggregate_stream() && !ctx.stream {
        let agg = aggregate_buffered_stream(channel, plan.target_kind(), &b);
        return Ok(ResponseBody::Full(transform_step::aggregate_response_body(
            plan, agg,
        )?));
    }
    if !status.is_success() || !plan.is_transform() {
        return Ok(ResponseBody::Full(b));
    }
    if ctx.stream {
        // buffered streaming (wasm): convert the whole SSE body
        let t = transform_step::stream_transformer(plan).expect("transform plan");
        Ok(ResponseBody::Full(Bytes::from(
            crate::transform::stream_adapter::convert_buffered(t, &b),
        )))
    } else {
        Ok(ResponseBody::Full(transform_step::response_body(plan, b)?))
    }
}

/// Collapse a buffered upstream event-stream into one response object: run the
/// channel's stream decoder over the whole body (kiro Smithy→Responses SSE;
/// codex has none → already SSE), then fold the SSE into a single JSON of the
/// target wire `kind`. Returns the body unchanged when the target is not a
/// content-generation kind.
fn aggregate_buffered_stream(
    channel: &Arc<dyn Channel>,
    kind: Option<ContentGenerationKind>,
    body: &Bytes,
) -> Bytes {
    let Some(kind) = kind else {
        return body.clone();
    };
    let sse = decode_buffered_stream(channel, body);
    Bytes::from(crate::transform::stream_adapter::aggregate_buffered(
        kind, &sse,
    ))
}

/// Run the channel's stream decoder over a whole buffered body (kiro Smithy
/// binary event-stream → canonical SSE; codex/none → bytes unchanged). This is
/// the "post channel-decode" provider response form — what §8-D upstream
/// capture must log (NOT the raw binary frames), matching the streaming arm.
fn decode_buffered_stream(channel: &Arc<dyn Channel>, body: &Bytes) -> Vec<u8> {
    match channel.stream_decoder() {
        Some(mut dec) => {
            let mut out = dec.push(body);
            out.extend(dec.finish());
            out
        }
        None => body.to_vec(),
    }
}
