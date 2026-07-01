//! Single-candidate attempt mechanics for the failover loop: build the request
//! parts, `prepare`, send, classify (and the refresh-failure / body
//! materialization helpers). Split out of `mod.rs` so the loop stays readable
//! and each file stays under the line cap.

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::Value;

use crate::app::AppState;
use crate::channel::{Channel, Disposition, PrepareCtx, ShapeCtx};
use crate::http::client::{ClientError, UpstreamClient};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::pipeline::health_hooks;
use crate::pipeline::outcome::ResponseBody;
use crate::pipeline::settle;
use crate::pipeline::transform::{self as transform_step, AttemptMemo, TransformPlan};
use crate::protocol::ContentGenerationKind;

/// Uniform per-attempt response body source. `Streaming` is native-only; on wasm
/// the executor always buffers, so `classify` runs identically on status+headers
/// regardless of mode and only the tail materialization branches.
pub enum BodySource {
    Buffered(Bytes),
    #[cfg(not(target_arch = "wasm32"))]
    Streaming(crate::http::client::RespStream),
}

/// One upstream attempt's outcome: the classified disposition plus everything
/// the success branch (body) and the failover-audit branch (wire facts) need.
/// Returned by [`attempt`]; health is recorded by the CALLER on the FINAL
/// disposition so an AuthDead retry doesn't cool the credential prematurely.
pub(super) struct AttemptOutcome {
    pub(super) status: StatusCode,
    pub(super) headers: HeaderMap,
    pub(super) source: BodySource,
    pub(super) disposition: Disposition,
    pub(super) send_ms: Option<f64>,
    /// Absolute upstream URL actually sent (failed-attempt audit rows).
    pub(super) sent_url: String,
    /// Upstream-shaped body actually sent (feeds the count ladder on success).
    pub(super) sent_body: Bytes,
    /// Wire method (audit rows).
    pub(super) method: Method,
    /// Upstream request headers actually sent — captured only when the
    /// upstream-log toggle is on (§8-D), `None` otherwise.
    pub(super) sent_headers: Option<HeaderMap>,
    /// This attempt was a `Custom` multi-step exchange (chatgpt image gen): its
    /// per-call §8-D logging is done inline by the [`CapturingClient`], so the
    /// caller skips the single aggregate `log_upstream` row.
    ///
    /// [`CapturingClient`]: crate::pipeline::capture::CapturingClient
    pub(super) multi_step: bool,
}

/// Run ONE upstream attempt for `cand` with `secret`: build the request parts,
/// `prepare`, send, and `classify`. Returns the classified outcome (caller
/// records health on the FINAL disposition). The unconditional failure paths
/// (request build, prepare, client config, transport) record health + audit
/// HERE and return `Err` — they are never retried via refresh, so the caller
/// only sets `last_err` and advances.
#[allow(clippy::too_many_arguments)]
pub(super) async fn attempt(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    channel: &Arc<dyn Channel>,
    secret: &Value,
    plan: &TransformPlan,
    rules: Option<&[crate::process::CompiledRule]>,
    memo: &mut AttemptMemo,
) -> Result<AttemptOutcome, PipelineError> {
    // request_parts is memoized per (target, model) — re-running it on the
    // AuthDead retry returns the same (cached) body; cheap and idempotent. A
    // build/transform error is config, not a key fault — no health record.
    let mut parts = transform_step::request_parts(ctx, cand, plan, rules, memo)?;

    // Channel REQUEST 整形 before prepare: field hygiene + header-token removal.
    // Mutates the headers that flow into PrepareCtx. Idempotent, so re-running on
    // the AuthDead retry is harmless.
    let shape = ShapeCtx {
        op: plan.shape_op(ctx),
        stream: ctx.stream,
        status: StatusCode::OK,
        enable_magic_cache: cand
            .provider
            .settings_json
            .get("enable_magic_cache")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    };
    let mut req_headers = parts.headers.take().unwrap_or_else(|| ctx.headers.clone());
    parts.body = channel.shape_request(parts.body, &mut req_headers, &shape);
    parts.headers = Some(req_headers);

    let prepared = match channel.prepare(PrepareCtx {
        secret,
        provider_settings: &cand.provider.settings_json,
        upstream_model_id: &cand.upstream_model_id,
        method: parts.method.clone(),
        path: &parts.path,
        query: parts.query.as_deref(),
        headers: parts.headers.as_ref().unwrap_or(&ctx.headers),
        body: parts.body,
    }) {
        Ok(p) => p,
        Err(e) => {
            // Prepare failures count against health like transient errors.
            health_hooks::record_failure(state, cand);
            return Err(PipelineError::Channel(e));
        }
    };

    // §17: capture what the wire actually carries — the sent body feeds the
    // count ladder; the URL feeds failed-attempt audit rows. A `Direct` request
    // carries this + is sent once. A `Custom` multi-step exchange (chatgpt image
    // gen) has NO single request — its `CapturingClient` logs each call — so the
    // audit fields are minimal and it carries the closure instead.
    let method = parts.method.clone();
    #[cfg(not(target_arch = "wasm32"))]
    let mut custom_stream_send = None;
    let (direct_req, custom_send, sent_body, sent_url, sent_headers) = match prepared {
        crate::channel::PreparedRequest::Direct(req) => {
            let sent_body = req.body().clone();
            let sent_url = req.uri().to_string();
            // §8-D upstream capture: clone the prepared headers only when the
            // toggle is on (redaction happens at write time in `capture`).
            let sent_headers = state
                .cp()
                .log_settings
                .enable_upstream_log
                .then(|| req.headers().clone());
            (Some(req), None, sent_body, sent_url, sent_headers)
        }
        crate::channel::PreparedRequest::Custom(send) => {
            (None, Some(send), Bytes::new(), String::new(), None)
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::channel::PreparedRequest::CustomStream(send) => {
            custom_stream_send = Some(send);
            (None, None, Bytes::new(), String::new(), None)
        }
    };

    // §7.4 effective (proxy, fingerprint) per attempt → pooled client; an
    // unusable target config (malformed proxy URL, fingerprint yielding no
    // emulation) fails THIS candidate like an upstream connect error — never a
    // silent downgrade to the default client, which would bypass the
    // proxy/TLS-profile policy.
    let client =
        match state.upstream_client_for_credential(channel, &cand.credential, &cand.provider) {
            Ok(c) => c,
            Err(e) => {
                health_hooks::record_failure(state, cand);
                settle::audit_failure(
                    state,
                    &ctx.request_id,
                    cand,
                    settle::FailedAttempt {
                        url: &sent_url,
                        method: method.as_str(),
                        status: 0,
                        latency_ms: 0,
                        error: &e.to_string(),
                    },
                );
                return Err(PipelineError::Transport(e.to_string()));
            }
        };

    #[cfg(not(target_arch = "wasm32"))]
    let send_started = std::time::Instant::now();

    let multi_step = custom_send.is_some();
    #[cfg(not(target_arch = "wasm32"))]
    let multi_step = multi_step || custom_stream_send.is_some();
    let make_capturing = || -> Arc<dyn UpstreamClient> {
        Arc::new(crate::pipeline::capture::CapturingClient::new(
            Arc::clone(&client),
            state.clone(),
            ctx.request_id.clone(),
            cand.provider.id,
            cand.credential.id,
        ))
    };
    let send_result: Result<(StatusCode, HeaderMap, BodySource), String> = 'send: {
        // Streaming custom exchange (chatgpt conduit): the body streams to the
        // client as the turn unfolds (vital for multi-minute deep research).
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(send) = custom_stream_send {
            break 'send send(make_capturing())
                .await
                .map(|(status, headers, st)| (status, headers, BodySource::Streaming(st)))
                .map_err(|e| e.to_string());
        }
        // Buffered custom multi-step exchange (chatgpt image gen): wrap the
        // resolved client so EVERY call it makes is captured (§8-D), then run it.
        if let Some(send) = custom_send {
            break 'send send(make_capturing())
                .await
                .map(|resp| {
                    let (p, b) = resp.into_parts();
                    (p.status, p.headers, BodySource::Buffered(b))
                })
                .map_err(|e| e.to_string());
        }
        send_once(
            client.as_ref(),
            direct_req.expect("a Direct prepared request"),
            ctx.stream,
        )
        .await
        .map_err(|e| e.to_string())
    };
    let (status, headers, source) = match send_result {
        Ok(t) => t,
        Err(e) => {
            health_hooks::record_failure(state, cand);
            settle::audit_failure(
                state,
                &ctx.request_id,
                cand,
                settle::FailedAttempt {
                    url: &sent_url,
                    method: method.as_str(),
                    status: 0,
                    latency_ms: 0,
                    error: &e,
                },
            );
            return Err(PipelineError::Transport(e));
        }
    };

    // Send latency feeds the member EWMA (native only; wasm has no
    // monotonic clock worth trusting here).
    #[cfg(not(target_arch = "wasm32"))]
    let send_ms = Some(send_started.elapsed().as_secs_f64() * 1000.0);
    #[cfg(target_arch = "wasm32")]
    let send_ms = None;

    let disposition = match &source {
        BodySource::Buffered(b) => channel.classify(status, &headers, b),
        #[cfg(not(target_arch = "wasm32"))]
        BodySource::Streaming(_) => channel.classify(status, &headers, &Bytes::new()),
    };

    Ok(AttemptOutcome {
        status,
        headers,
        source,
        disposition,
        send_ms,
        sent_url,
        sent_body,
        method,
        sent_headers,
        multi_step,
    })
}

/// §14.5 refresh failure handling at the lazy pre-use seam: cool the credential
/// (auth-dead semantics) + persist the edge + audit, mirroring an AuthDead
/// classification so a bad refresh removes the credential from rotation.
pub(super) fn refresh_failed(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    e: &crate::channel::ChannelError,
) {
    tracing::warn!(
        credential_id = cand.credential.id,
        error = %e,
        "credential refresh failed; cooling credential"
    );
    health_hooks::record_attempt(state, cand, &Disposition::AuthDead, None);
    settle::audit_failure(
        state,
        &ctx.request_id,
        cand,
        settle::FailedAttempt {
            url: "",
            method: ctx.method.as_str(),
            status: 0,
            latency_ms: 0,
            error: &format!("refresh failed: {e}"),
        },
    );
}

/// Output of [`materialize`]: the client-facing body plus, for non-streaming
/// responses, the captured upstream (provider) response body (§8-D). Streaming
/// upstream capture is handled inline by the spliced `capture_raw_stream` guard,
/// so `upstream_raw` is `None` for streams.
pub(super) struct Materialized {
    pub body: ResponseBody,
    pub upstream_raw: Option<Bytes>,
    pub settle: Option<BufferedSettle>,
}

pub(super) struct BufferedSettle {
    pub ctx: settle::SettleCtx,
    pub body: Bytes,
    pub stream: bool,
}

/// What [`materialize`] needs to capture a streaming upstream response body.
/// `Some` only when upstream response-body logging is enabled. (On wasm there is
/// no streaming arm, so the fields are constructed for the gating check only.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) struct UpstreamRespCapture {
    pub state: AppState,
    pub request_id: String,
}

pub(super) struct ResponseRuleCtx<'a> {
    pub rules: &'a [crate::process::CompiledRule],
    pub model: &'a str,
}

/// Materialize an attempt's body. Response-direction transform applies only to
/// 2xx bodies — error payloads stay provider-native (M2 fidelity note). When
/// `upstream` is `Some`, the post-decode provider response is captured for
/// §8-D logging (buffered: returned via `upstream_raw`; streaming: backfilled by
/// the spliced guard).
#[allow(clippy::too_many_arguments)]
pub(super) fn materialize(
    channel: &Arc<dyn Channel>,
    source: BodySource,
    plan: &TransformPlan,
    ctx: &RequestCtx,
    status: StatusCode,
    response_rules: Option<ResponseRuleCtx<'_>>,
    upstream: Option<UpstreamRespCapture>,
    settle_ctx: Option<settle::SettleCtx>,
) -> Result<Materialized, PipelineError> {
    match source {
        BodySource::Buffered(b) => {
            let shape = ShapeCtx {
                op: plan.shape_op(ctx),
                stream: ctx.stream,
                status,
                enable_magic_cache: false,
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
            let settle_stream = ctx.stream || plan.is_aggregate_stream();
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
        #[cfg(not(target_arch = "wasm32"))]
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

/// The buffered-body conversion ladder, split out so [`materialize`] stays
/// focused on capture + stream wiring.
fn materialize_buffered(
    channel: &Arc<dyn Channel>,
    plan: &TransformPlan,
    ctx: &RequestCtx,
    status: StatusCode,
    b: Bytes,
) -> Result<ResponseBody, PipelineError> {
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

/// One upstream send → uniform `(status, headers, BodySource)`. Streaming on
/// native when requested; always buffered on wasm.
#[cfg(not(target_arch = "wasm32"))]
async fn send_once(
    client: &dyn UpstreamClient,
    req: http::Request<Bytes>,
    stream: bool,
) -> Result<(StatusCode, HeaderMap, BodySource), ClientError> {
    if stream {
        let (status, headers, st) = client.send_streaming(req).await?;
        Ok((status, headers, BodySource::Streaming(st)))
    } else {
        let resp = client.send(req).await?;
        let (parts, body) = resp.into_parts();
        Ok((parts.status, parts.headers, BodySource::Buffered(body)))
    }
}

#[cfg(target_arch = "wasm32")]
async fn send_once(
    client: &dyn UpstreamClient,
    req: http::Request<Bytes>,
    _stream: bool,
) -> Result<(StatusCode, HeaderMap, BodySource), ClientError> {
    let resp = client.send(req).await?;
    let (parts, body) = resp.into_parts();
    Ok((parts.status, parts.headers, BodySource::Buffered(body)))
}
