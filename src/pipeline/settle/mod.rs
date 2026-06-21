//! §17 stream settlement: bounded relay buffer on the hot path (refcounted
//! `Bytes` clones, zero parse), exactly-once settle via explicit end OR Drop
//! guard, and the inline settle for buffered bodies. The counting ladder
//! lives in [`ladder`]; frame decoding/error frames in [`frames`].
//!
//! ~390 lines by design (M6 Task 3/4 budget: settle may exceed the 200-line
//! ideal; hard cap respected). Quota/counter reconciliation lives in
//! [`reconcile`]; the counting ladder in [`ladder`]; frames in [`frames`].

pub(crate) mod frames;
mod ladder;
pub(crate) mod provider;
mod reconcile;

use ladder::count_and_record;
#[cfg(not(target_arch = "wasm32"))]
use ladder::ladder;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::app::AppState;
use crate::billing::{self, UsageRecord};
use crate::channel::Channel;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::outcome::ResponseBody;
use crate::protocol::{ContentGenerationKind, OperationKind, Provider as Family};
use crate::store::persistence::records::{Credential, Provider as ProviderRecord, Scope};
use crate::usage::{Ended, NormalizedUsage, UsageSource, extract};
use crate::util::time::unix_now;

/// Everything one settle needs, captured at success time so the (possibly
/// detached) settle task never touches the control-plane snapshot. Secrets
/// stay SEALED — `credential.secret_json` is opened at count time only.
pub struct SettleCtx {
    state: AppState,
    request_id: String,
    at: i64,
    route_name: Option<String>,
    org_id: Option<i64>,
    team_id: Option<i64>,
    user_id: Option<i64>,
    user_key_id: Option<i64>,
    operation: String,
    kind: String,
    /// Upstream member model (`cand.upstream_model_id`).
    model: String,
    /// Inbound wire kind — the relayed bytes are inbound-shaped by the time
    /// they reach the buffer (post response-transform).
    inbound: ContentGenerationKind,
    /// Channel's native family — drives the upstream-count ladder rung.
    /// (wasm: ladder is local-only, so this and `channel` are unread there.)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    upstream_family: Family,
    /// The upstream-shaped request body actually sent (refcounted clone).
    request_body: Bytes,
    /// Resolved at capture time from `models_by_provider`.
    pricing: billing::price::Pricing,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    channel: Arc<dyn Channel>,
    provider: Arc<ProviderRecord>,
    credential: Arc<Credential>,
    /// §17 pre-deducted pending (micro-dollars) to refund exactly at settle.
    pending_micros: i64,
    /// §15.3: upstream latency (ms, time-to-first-response) of this attempt;
    /// 0 when unmeasured (wasm has no monotonic clock).
    latency_ms: i64,
    /// Scopes with a quota row, resolved at capture time (reconcile targets).
    quota_scopes: Vec<(Scope, i64)>,
    /// rate_limits row ids with a `total_tokens` budget matching this
    /// request's route/provider name — fed via `rlt:{id}:d{day}` at settle.
    token_rlt_ids: Vec<i64>,
}

impl SettleCtx {
    /// Capture billing context for a successful attempt. `None` = nothing to
    /// settle (non-content op).
    pub fn capture(
        state: &AppState,
        ctx: &RequestCtx,
        cand: &Candidate,
        channel: &Arc<dyn Channel>,
        request_body: Bytes,
        latency_ms: i64,
    ) -> Option<Self> {
        let op = ctx.op?;
        let OperationKind::ContentGeneration(inbound) = op.kind else {
            return None; // models/count/etc. are never billed (§17)
        };
        let identity = ctx.identity.as_deref();
        // Resolve everything reconcile needs NOW (snapshot guard scoped to
        // this block — the detached settle task never touches the snapshot).
        let (pricing, quota_scopes, token_rlt_ids) = {
            let cp = state.cp();
            let pricing = crate::billing::pending::model_pricing(
                &cp,
                cand.provider.id,
                &cand.upstream_model_id,
            );
            let name = ctx.route_name.as_deref().unwrap_or(&cand.provider.name);
            let (scopes, rlt_ids) = match identity {
                Some(i) => (
                    crate::pipeline::authz::quota_scopes(&cp, i),
                    crate::pipeline::authz::token_limit_ids(&cp, i, name),
                ),
                None => (Vec::new(), Vec::new()),
            };
            (pricing, scopes, rlt_ids)
        };
        Some(Self {
            state: state.clone(),
            request_id: ctx.request_id.clone(),
            at: unix_now(),
            route_name: ctx.route_name.clone(),
            org_id: identity.map(|i| i.user.org_id),
            team_id: identity.and_then(|i| i.user.team_id),
            user_id: identity.map(|i| i.user.id),
            user_key_id: identity.map(|i| i.user_key.id),
            operation: enum_str(&op.operation),
            kind: enum_str(&op.kind),
            model: cand.upstream_model_id.clone(),
            inbound,
            upstream_family: channel.provider_family(),
            request_body,
            pricing,
            channel: Arc::clone(channel),
            provider: Arc::clone(&cand.provider),
            credential: Arc::clone(&cand.credential),
            pending_micros: ctx.pending_micros,
            latency_ms,
            quota_scopes,
            token_rlt_ids,
        })
    }
}

/// Wire settlement onto a materialized body: streams get the relay
/// buffer + Drop guard; full bodies settle inline (`Complete`).
pub async fn attach(ctx: Option<SettleCtx>, body: ResponseBody, stream: bool) -> ResponseBody {
    let Some(ctx) = ctx else { return body };
    match body {
        ResponseBody::Full(b) => {
            settle_full(ctx, &b, stream).await;
            ResponseBody::Full(b)
        }
        #[cfg(not(target_arch = "wasm32"))]
        ResponseBody::Stream(s) => ResponseBody::Stream(
            crate::pipeline::stream::instrument_stream(s, StreamGuard::new(ctx)),
        ),
    }
}

/// Inline settle for a fully-buffered body (non-streaming, or wasm's buffered
/// streaming). Usage-in-body is the fast path; a miss falls to the counting
/// ladder (spawned on native so the response isn't delayed).
async fn settle_full(ctx: SettleCtx, body: &Bytes, stream: bool) {
    let extracted = if stream {
        extract::from_stream_frames(ctx.inbound, &frames::decode(body))
    } else {
        serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|v| extract::from_response(ctx.inbound.provider(), &v))
    };
    match extracted {
        Some(u) => record(&ctx, u, UsageSource::Upstream, Ended::Complete).await,
        None => {
            let text = if stream {
                frames::produced_text(ctx.inbound, &frames::decode(body))
            } else {
                crate::tokenize::harvest(body).0.join("\n")
            };
            #[cfg(not(target_arch = "wasm32"))]
            tokio::spawn(count_and_record(ctx, text, Ended::Complete));
            #[cfg(target_arch = "wasm32")]
            count_and_record(ctx, text, Ended::Complete).await;
        }
    }
}

// ── bounded relay buffer + Drop guard (native streaming only) ────────────────

#[cfg(not(target_arch = "wasm32"))]
const BUFFER_CAP: usize = 4 << 20;
#[cfg(not(target_arch = "wasm32"))]
const TAIL_KEEP: usize = 64 << 10;

/// Bounded chunk buffer: beyond ~4MB the oldest chunks are dropped, but the
/// most recent ~64KB tail (where final usage frames live) and the running
/// relayed-byte total are always kept. Shared with `pipeline::stream` for
/// upstream raw-response capture (§8-D).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct RelayBuffer {
    chunks: std::collections::VecDeque<Bytes>,
    stored: usize,
    total: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl RelayBuffer {
    pub(crate) fn new() -> Self {
        Self {
            chunks: std::collections::VecDeque::new(),
            stored: 0,
            total: 0,
        }
    }

    pub(crate) fn push(&mut self, chunk: Bytes) {
        self.total += chunk.len() as u64;
        self.stored += chunk.len();
        self.chunks.push_back(chunk);
        while self.stored > BUFFER_CAP {
            let Some(front) = self.chunks.front() else {
                break;
            };
            if self.stored - front.len() < TAIL_KEEP {
                break; // never drop into the tail window
            }
            let dropped = self.chunks.pop_front().expect("front exists");
            self.stored -= dropped.len();
        }
    }

    pub(crate) fn concat(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.stored);
        for c in &self.chunks {
            out.extend_from_slice(c);
        }
        out
    }

    /// Like [`concat`](Self::concat) but, when earliest chunks were dropped
    /// (the stream exceeded `BUFFER_CAP`), prepends a visible truncation marker
    /// so §8-D body logs don't silently look complete. Usage extraction keeps
    /// using `concat` (it must not inject text into the frame stream).
    pub(crate) fn concat_for_log(&self) -> Vec<u8> {
        let dropped = self.total.saturating_sub(self.stored as u64);
        let body = self.concat();
        if dropped == 0 {
            return body;
        }
        let mut out = format!(
            "…[truncated: {dropped} earliest bytes dropped — streamed response exceeded the {} MiB capture buffer]\n",
            BUFFER_CAP >> 20
        )
        .into_bytes();
        out.extend_from_slice(&body);
        out
    }
}

/// Exactly-once settle guard for a relayed stream: explicit [`finish`]
/// (normal end) or Drop (client/upstream break = `Interrupted`) spawns the
/// settle task; whichever fires first wins.
///
/// [`finish`]: StreamGuard::finish
#[cfg(not(target_arch = "wasm32"))]
pub struct StreamGuard {
    inner: Option<(SettleCtx, RelayBuffer)>,
}

#[cfg(not(target_arch = "wasm32"))]
impl StreamGuard {
    pub fn new(ctx: SettleCtx) -> Self {
        Self {
            inner: Some((ctx, RelayBuffer::new())),
        }
    }

    /// Buffer one relayed chunk (refcounted clone — zero copy, zero parse).
    pub fn push(&mut self, chunk: &Bytes) {
        if let Some((_, buf)) = self.inner.as_mut() {
            buf.push(chunk.clone());
        }
    }

    pub fn inbound_kind(&self) -> Option<ContentGenerationKind> {
        self.inner.as_ref().map(|(c, _)| c.inbound)
    }

    /// Explicit normal end — settles `Complete`.
    pub fn finish(mut self) {
        self.complete(Ended::Complete);
    }

    fn complete(&mut self, ended: Ended) {
        let Some((ctx, buf)) = self.inner.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                h.spawn(settle_stream(ctx, buf, ended));
            }
            Err(_) => {
                tracing::warn!(request_id = %ctx.request_id, "no runtime at settle; usage dropped");
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for StreamGuard {
    fn drop(&mut self) {
        self.complete(Ended::Interrupted);
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn settle_stream(ctx: SettleCtx, buf: RelayBuffer, ended: Ended) {
    let bytes = buf.concat();
    tracing::debug!(
        request_id = %ctx.request_id,
        relayed_bytes = buf.total,
        buffered_bytes = bytes.len(),
        ended = %ended,
        "settling stream"
    );
    let frames = frames::decode(&bytes);
    let (usage, source) = match ended {
        // normal end: trust the upstream-reported final usage when present
        Ended::Complete => match extract::from_stream_frames(ctx.inbound, &frames) {
            Some(u) => (u, UsageSource::Upstream),
            None => ladder(&ctx, &frames::produced_text(ctx.inbound, &frames)).await,
        },
        // abnormal end: bill the PRODUCED part via the counting ladder (§17)
        Ended::Interrupted => ladder(&ctx, &frames::produced_text(ctx.inbound, &frames)).await,
    };
    record(&ctx, usage, source, ended).await;
    // §8-D: backfill the captured downstream response body (the row was
    // appended at execute() return; this UPDATE lands after it). Gated inside.
    // `concat_for_log` surfaces head-drop truncation on >4MB streams.
    crate::pipeline::capture::record_downstream_response(
        &ctx.state,
        &ctx.request_id,
        &buf.concat_for_log(),
    )
    .await;
}

// ── recording ────────────────────────────────────────────────────────────────

async fn record(ctx: &SettleCtx, usage: NormalizedUsage, source: UsageSource, ended: Ended) {
    let cost = billing::price::cost(&usage, &ctx.pricing);
    // §17 reconcile first (pending refund + quota cost_used + counter feeds):
    // the usage row may be idempotently skipped, but the settle path runs
    // exactly once per request (StreamGuard / inline), so this never doubles.
    reconcile::reconcile(ctx, &usage, cost).await;
    // §8-E: `enable_usage` gates the RECORDING only — reconcile above is
    // billing correctness (pending refund / quota feed) and always runs.
    if !ctx.state.cp().log_settings.enable_usage {
        return;
    }
    let rec = UsageRecord {
        request_id: &ctx.request_id,
        at: ctx.at,
        route_name: ctx.route_name.as_deref(),
        provider_id: Some(ctx.provider.id),
        credential_id: Some(ctx.credential.id),
        org_id: ctx.org_id,
        team_id: ctx.team_id,
        user_id: ctx.user_id,
        user_key_id: ctx.user_key_id,
        operation: &ctx.operation,
        kind: &ctx.kind,
        model: Some(&ctx.model),
        usage: &usage,
        cost,
        latency_ms: ctx.latency_ms,
        source,
        ended,
    };
    if let Err(e) = billing::record_success(ctx.state.persistence.as_ref(), rec).await {
        tracing::warn!(request_id = %ctx.request_id, error = %e, "usage settle write failed");
    }
}

/// One failed failover attempt's wire facts, for the audit row.
pub struct FailedAttempt<'a> {
    pub url: &'a str,
    pub method: &'a str,
    pub status: i64,
    pub latency_ms: i64,
    pub error: &'a str,
}

/// Audit one failed failover attempt (`upstream_requests`, never billed).
/// Gated by `enable_upstream_log` (§8-D/§8-E). Fire-and-forget on native;
/// skipped on wasm (no detached tasks).
pub fn audit_failure(state: &AppState, request_id: &str, cand: &Candidate, a: FailedAttempt<'_>) {
    if !state.cp().log_settings.enable_upstream_log {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let persistence = Arc::clone(&state.persistence);
        let (provider_id, credential_id) = (cand.provider.id, cand.credential.id);
        let (status, latency_ms) = (a.status, a.latency_ms);
        let at = unix_now();
        let (request_id, url, method, error) = (
            request_id.to_owned(),
            a.url.to_owned(),
            a.method.to_owned(),
            a.error.to_owned(),
        );
        tokio::spawn(async move {
            let rec = billing::FailureRecord {
                request_id: &request_id,
                at,
                provider_id: Some(provider_id),
                credential_id: Some(credential_id),
                url: &url,
                method: &method,
                status,
                latency_ms,
                error: &error,
            };
            if let Err(e) = billing::record_failure(persistence.as_ref(), rec).await {
                tracing::warn!(error = %e, "failed-attempt audit write failed");
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (state, request_id, cand, a);
}

/// snake_case wire string of a serde unit-enum value.
fn enum_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
