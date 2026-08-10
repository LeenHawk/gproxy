//! §17 settlement for the provider-shaped billable ops that are NOT
//! content-generation: compact content, embeddings, rerank, and image
//! generation. Buffered JSON responses settle inline; image SSE responses use
//! a bounded relay guard so they can stream without losing final usage. The
//! content-generation settle path ([`super::SettleCtx`]) is untouched.
//!
//! Pricing uses the normalized per-million-token rates from response `usage`.
//! Image operations require upstream token usage as well; a response without
//! it settles at zero rather than guessing from the number of returned images.

use bytes::Bytes;
use serde_json::Value;

use crate::app::AppState;
use crate::billing::{self, UsageRecord, price};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::{Operation, OperationKey, Provider as Family};
use crate::usage::{Ended, UsageSource, extract};
use crate::util::time::unix_now;

mod image_sse;
mod stream;
pub(crate) use stream::StreamGuard;

/// Whether this op settles here (compact / embeddings / rerank / images). Lets the
/// caller skip spawning a settle task for every other buffered success.
pub(crate) fn billable(op: Option<OperationKey>) -> bool {
    matches!(
        op.map(|o| o.operation()),
        Some(
            Operation::CompactContent
                | Operation::CreateEmbedding
                | Operation::Rerank
                | Operation::CreateImage
                | Operation::EditImage
        )
    )
}

/// Provider settlement facts fixed before a response body is detached or
/// exposed as a stream. This keeps pricing, pending refunds, and limiter targets
/// stable if the control plane changes while a long image stream is in flight.
pub(super) struct Captured {
    pub(super) state: AppState,
    pub(super) ctx: RequestCtx,
    pub(super) cand: Candidate,
    usage_family: Family,
    pricing: billing::price::Pricing,
    quota_scopes: Vec<(crate::store::persistence::records::Scope, i64)>,
    token_rlt_ids: Vec<i64>,
}

impl Captured {
    pub(super) fn new(
        state: &AppState,
        ctx: &RequestCtx,
        cand: &Candidate,
        usage_family: Family,
    ) -> Self {
        let identity = ctx.identity.as_deref();
        let (pricing, quota_scopes, token_rlt_ids) = {
            let cp = state.cp();
            let pricing =
                billing::pending::resolve_pricing(&cp, cand.provider.id, &cand.upstream_model_id)
                    .pricing;
            let (scopes, token_rlt_ids) = identity.map_or_else(
                || (Vec::new(), Vec::new()),
                |identity| {
                    let name = ctx.route_name.as_deref().unwrap_or(&cand.provider.name);
                    (
                        crate::pipeline::authz::quota_scopes(&cp, identity),
                        crate::pipeline::authz::token_limit_ids(&cp, identity, name),
                    )
                },
            );
            (pricing, scopes, token_rlt_ids)
        };
        Self {
            state: state.clone(),
            ctx: ctx.clone(),
            cand: cand.clone(),
            usage_family,
            pricing,
            quota_scopes,
            token_rlt_ids,
        }
    }
}

/// Detach provider-op settlement on native; edge request contexts must await it.
pub(crate) async fn schedule(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    body: Bytes,
    usage_family: Family,
) {
    let captured = Captured::new(state, ctx, cand, usage_family);
    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::spawn(async move {
            settle_ended(&captured, &body, Ended::Complete).await;
        });
    }
    #[cfg(target_arch = "wasm32")]
    settle_ended(&captured, &body, Ended::Complete).await;
}

async fn settle_ended(captured: &Captured, body: &Bytes, ended: Ended) {
    let Captured {
        state,
        ctx,
        cand,
        usage_family,
        pricing,
        quota_scopes,
        token_rlt_ids,
    } = captured;
    let Some(op) = ctx.op else { return };
    let is_embedding = matches!(op.operation(), Operation::CreateEmbedding);
    let is_rerank = matches!(op.operation(), Operation::Rerank);
    let is_compact = matches!(op.operation(), Operation::CompactContent);
    let is_image = matches!(
        op.operation(),
        Operation::CreateImage | Operation::EditImage
    );
    if !is_embedding && !is_rerank && !is_compact && !is_image {
        return;
    }

    let identity = ctx.identity.as_deref();
    let (parsed, parse_error): (Option<Value>, Option<serde_json::Error>) =
        match serde_json::from_slice(body) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
    // Image responses may arrive as a buffered transcript or through the
    // streaming relay guard. Decode the completed event in either case.
    let stream_frames = (is_image && parsed.is_none())
        .then(|| super::frames::decode(body).ok())
        .flatten()
        .filter(|frames| !frames.is_empty());
    if parsed.is_none() && stream_frames.is_none() {
        tracing::warn!(
            request_id = %ctx.request_id,
            provider = %cand.provider.name,
            upstream_model = %cand.upstream_model_id,
            error = %parse_error.expect("failed JSON parse has an error"),
            "billable provider response parse failed; using zero usage"
        );
    }
    let extracted = parsed
        .as_ref()
        .and_then(|value| {
            if is_image {
                extract::from_image_response(*usage_family, value)
            } else if is_rerank {
                extract::from_rerank_response(*usage_family, value)
            } else {
                extract::from_response(*usage_family, value)
            }
        })
        .or_else(|| {
            stream_frames
                .as_deref()
                .and_then(|frames| extract::from_image_stream_frames(*usage_family, frames))
        });
    if (parsed.is_some() || stream_frames.is_some()) && extracted.is_none() {
        let operation = if is_compact {
            "compact_content"
        } else if is_embedding {
            "create_embedding"
        } else if is_rerank {
            "rerank"
        } else if matches!(op.operation(), Operation::EditImage) {
            "edit_image"
        } else {
            "create_image"
        };
        tracing::warn!(
            request_id = %ctx.request_id,
            provider = %cand.provider.name,
            upstream_model = %cand.upstream_model_id,
            operation,
            "provider usage missing; using zero usage"
        );
    }
    let source = if is_compact && extracted.is_none() {
        UsageSource::Estimated
    } else {
        UsageSource::Upstream
    };
    let usage = extracted.unwrap_or_default();
    let cost = price::cost(&usage, pricing);

    // Keep provider-shaped operations on the same billing and token-counter
    // path as content generation. Recording may be disabled, but quota and
    // limiter reconciliation is always required.
    super::reconcile::reconcile_target(
        super::reconcile::ReconcileTarget {
            state,
            request_id: &ctx.request_id,
            credential_id: cand.credential.id,
            pending_micros: ctx.pending_micros,
            quota_scopes,
            token_rlt_ids,
        },
        &usage,
        cost,
    )
    .await;

    let operation = super::enum_str(&op.operation());
    let kind = super::enum_str(&op.kind());
    let rec = UsageRecord {
        request_id: &ctx.request_id,
        at: unix_now(),
        route_name: ctx.route_name.as_deref(),
        provider_id: Some(cand.provider.id),
        credential_id: Some(cand.credential.id),
        org_id: identity.map(|i| i.user.org_id),
        team_id: identity.and_then(|i| i.user.team_id),
        user_id: identity.map(|i| i.user.id),
        user_key_id: identity.map(|i| i.user_key.id),
        operation: &operation,
        kind: &kind,
        model: Some(&cand.upstream_model_id),
        usage: &usage,
        cost,
        latency_ms: 0,
        source,
        ended,
    };
    // §8-E: `enable_usage` gates the usage row only.
    if state.cp().log_settings.enable_usage
        && let Err(e) = billing::record_success(state.persistence.as_ref(), rec).await
    {
        tracing::warn!(request_id = %ctx.request_id, error = %e, "provider settle write failed");
    }
    tracing::debug!(
        request_id = %ctx.request_id,
        provider = %cand.provider.name,
        upstream_model = %cand.upstream_model_id,
        usage_source = %source,
        ended = %ended,
        image_output_tokens = usage.image_output,
        tokens = usage.total(),
        cost = %cost,
        "provider usage settled"
    );
}
