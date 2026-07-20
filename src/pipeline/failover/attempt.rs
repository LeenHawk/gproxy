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
use crate::pipeline::settle;
use crate::pipeline::transform::{self as transform_step, AttemptMemo, TransformPlan};

mod body;

pub(super) use body::{Materialized, ResponseRuleCtx, UpstreamRespCapture, materialize};

/// Uniform per-attempt response body source. Streaming is backed by wreq on
/// native and Fetch `ReadableStream` on wasm.
pub enum BodySource {
    Buffered(Bytes),
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
        stream: plan.upstream_stream(ctx),
        status: StatusCode::OK,
        settings: &cand.provider.settings_json,
    };
    let mut req_headers = parts.headers.take().unwrap_or_else(|| ctx.headers.clone());
    parts.body = channel.shape_request(parts.body, &mut req_headers, &shape);
    parts.headers = Some(req_headers);
    #[cfg(not(target_arch = "wasm32"))]
    let prepared_body = parts.body.clone();

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
            (None, None, prepared_body, String::new(), None)
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
            plan.upstream_stream(ctx),
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
    health_hooks::record_credential_attempt(
        state,
        &cand.provider,
        &cand.credential,
        &Disposition::AuthDead,
    );
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

/// One upstream send → uniform `(status, headers, BodySource)`.
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
