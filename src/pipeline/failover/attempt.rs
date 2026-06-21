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

    // §17: capture what the wire actually carries — the sent body feeds
    // the count ladder; the URL feeds failed-attempt audit rows. Both are
    // cheap (refcounted Bytes / one small String). Captured before client
    // resolution so a config-failure audit row carries the real URL/method.
    let sent_body = prepared.request.body().clone();
    let sent_url = prepared.request.uri().to_string();
    let method = parts.method.clone();
    // §8-D upstream capture: clone the prepared headers only when the toggle
    // is on (redaction happens at write time in `capture`).
    let sent_headers = state
        .cp()
        .log_settings
        .enable_upstream_log
        .then(|| prepared.request.headers().clone());

    // §7.4 effective (proxy, fingerprint) per attempt → pooled client; an
    // unusable target config (malformed proxy URL, fingerprint yielding no
    // emulation) fails THIS candidate like an upstream connect error — never a
    // silent downgrade to the default client, which would bypass the
    // proxy/TLS-profile policy. wasm and non-wreq builds always use the
    // default upstream client.
    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    let client = {
        let global_proxy = state.upstream_proxy_url();
        let proxy = crate::channel::resolve::effective_proxy(
            &cand.credential,
            &cand.provider,
            global_proxy.as_deref(),
        );
        let fingerprint =
            crate::channel::resolve::effective_tls_fingerprint(&cand.credential, &cand.provider);
        // DB fingerprint (credential/provider) wins; otherwise fall back to the
        // channel's built-in impersonation profile; otherwise the default client.
        let pool_result = if let Some(fp) = fingerprint.as_ref() {
            state.client_pool.for_target(proxy.as_deref(), Some(fp))
        } else if let Some(emu) = channel.default_emulation() {
            state
                .client_pool
                .for_channel(proxy.as_deref(), channel.id(), emu)
        } else {
            state.client_pool.for_target(proxy.as_deref(), None)
        };
        match pool_result {
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
        }
    };
    #[cfg(not(all(not(target_arch = "wasm32"), feature = "upstream-wreq")))]
    let client = Arc::clone(&state.upstream);

    #[cfg(not(target_arch = "wasm32"))]
    let send_started = std::time::Instant::now();

    let (status, headers, source) =
        match send_once(client.as_ref(), prepared.into_http(), ctx.stream).await {
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
                        error: &e.to_string(),
                    },
                );
                return Err(PipelineError::Transport(e.to_string()));
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
}

/// What [`materialize`] needs to capture a streaming upstream response body.
/// `Some` only when upstream response-body logging is enabled. (On wasm there is
/// no streaming arm, so the fields are constructed for the gating check only.)
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(super) struct UpstreamRespCapture {
    pub state: AppState,
    pub request_id: String,
}

/// Materialize an attempt's body. Response-direction transform applies only to
/// 2xx bodies — error payloads stay provider-native (M2 fidelity note). When
/// `upstream` is `Some`, the post-decode provider response is captured for
/// §8-D logging (buffered: returned via `upstream_raw`; streaming: backfilled by
/// the spliced guard).
pub(super) fn materialize(
    channel: &Arc<dyn Channel>,
    source: BodySource,
    plan: &TransformPlan,
    ctx: &RequestCtx,
    status: StatusCode,
    upstream: Option<UpstreamRespCapture>,
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
            let body = materialize_buffered(channel, plan, ctx, status, b)?;
            Ok(Materialized { body, upstream_raw })
        }
        #[cfg(not(target_arch = "wasm32"))]
        BodySource::Streaming(st) => {
            if !status.is_success() {
                // Streamed error: undecoded passthrough, no upstream capture.
                return Ok(Materialized {
                    body: ResponseBody::Stream(crate::pipeline::stream::into_byte_stream(st)),
                    upstream_raw: None,
                });
            }
            // Order: raw upstream → channel decoder (envelope/binary → canonical
            // provider SSE) → [§8-D raw capture tee] → M2 transform (provider →
            // inbound, or identity on passthrough) → client.
            let st = match channel.stream_decoder() {
                Some(dec) => crate::pipeline::stream::channel_decode_stream(st, dec),
                None => crate::pipeline::stream::into_byte_stream(st),
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
