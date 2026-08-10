//! §17 settlement for the provider-shaped billable ops that are NOT
//! content-generation: compact content, embeddings, and image generation. These
//! are always non-streaming single-JSON responses, so they settle inline from
//! the buffered body (no counting ladder, no stream guard). The
//! content-generation settle path ([`super::SettleCtx`]) is untouched.
//!
//! Pricing uses the normalized per-million-token rates from response `usage`.
//! Image operations require upstream token usage as well; a response without
//! it settles at zero rather than guessing from the number of returned images.

use bytes::Bytes;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::app::AppState;
use crate::billing::{self, UsageRecord, price};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::{Operation, OperationKey, Provider as Family};
use crate::usage::{Ended, UsageSource, extract};
use crate::util::time::unix_now;

/// Whether this op settles here (compact / embeddings / images). Lets the
/// caller skip spawning a settle task for every other buffered success.
pub(crate) fn billable(op: Option<OperationKey>) -> bool {
    matches!(
        op.map(|o| o.operation()),
        Some(
            Operation::CompactContent
                | Operation::CreateEmbedding
                | Operation::CreateImage
                | Operation::EditImage
        )
    )
}

/// Detach provider-op settlement on native; edge request contexts must await it.
pub(crate) async fn schedule(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    body: Bytes,
    usage_family: Family,
) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (state, ctx, cand) = (state.clone(), ctx.clone(), cand.clone());
        tokio::spawn(async move {
            settle(&state, &ctx, &cand, &body, usage_family).await;
        });
    }
    #[cfg(target_arch = "wasm32")]
    settle(state, ctx, cand, &body, usage_family).await;
}

/// Settle a successful compact / embedding / image response. No-op for any other
/// operation (the caller invokes this for every successful buffered response;
/// content-generation, models and count ops return early here).
pub(crate) async fn settle(
    state: &AppState,
    ctx: &RequestCtx,
    cand: &Candidate,
    body: &Bytes,
    usage_family: Family,
) {
    let Some(op) = ctx.op else { return };
    let is_embedding = matches!(op.operation(), Operation::CreateEmbedding);
    let is_compact = matches!(op.operation(), Operation::CompactContent);
    let is_image = matches!(
        op.operation(),
        Operation::CreateImage | Operation::EditImage
    );
    if !is_embedding && !is_compact && !is_image {
        return;
    }

    // Resolve pricing + quota scopes under a scoped snapshot guard (the await
    // below never touches the snapshot).
    let identity = ctx.identity.as_deref();
    let (pricing, quota_scopes) = {
        let cp = state.cp();
        let resolved =
            billing::pending::resolve_pricing(&cp, cand.provider.id, &cand.upstream_model_id);
        let pricing = resolved.pricing;
        let scopes = identity
            .map(|i| crate::pipeline::authz::quota_scopes(&cp, i))
            .unwrap_or_default();
        (pricing, scopes)
    };

    let (parsed, parse_error): (Option<Value>, Option<serde_json::Error>) =
        match serde_json::from_slice(body) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error)),
        };
    // Images supports `stream:true`, while the provider operation currently
    // reaches this settlement path as a buffered SSE transcript. Decode the
    // completed event instead of treating the transcript as malformed JSON.
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
                extract::from_image_response(usage_family, value)
            } else {
                extract::from_response(usage_family, value)
            }
        })
        .or_else(|| {
            stream_frames
                .as_deref()
                .and_then(|frames| extract::from_image_stream_frames(usage_family, frames))
        });
    if (parsed.is_some() || stream_frames.is_some()) && extracted.is_none() {
        let operation = if is_compact {
            "compact_content"
        } else if is_embedding {
            "create_embedding"
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
    let cost = price::cost(&usage, &pricing);

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
        ended: Ended::Complete,
    };
    // §8-E: `enable_usage` gates the usage row only — the reconcile below
    // (pending refund + quota cost) is billing correctness and always runs.
    if state.cp().log_settings.enable_usage
        && let Err(e) = billing::record_success(state.persistence.as_ref(), rec).await
    {
        tracing::warn!(request_id = %ctx.request_id, error = %e, "provider settle write failed");
    }
    // §17 reconcile, symmetric with the content-generation path: refund the
    // pre-deducted pending (charged in `execute`), then persist the actual cost
    // into each quota row. `refund` is a no-op when nothing was pre-deducted.
    billing::pending::refund(
        state.cache.as_ref(),
        &quota_scopes,
        ctx.pending_micros,
        &ctx.request_id,
    )
    .await;
    if cost > Decimal::ZERO {
        for (scope, scope_id) in &quota_scopes {
            if let Err(e) = state
                .persistence
                .add_quota_cost(*scope, *scope_id, cost)
                .await
            {
                tracing::warn!(request_id = %ctx.request_id, error = %e, "provider quota write failed");
            }
        }
    }
    tracing::debug!(
        request_id = %ctx.request_id,
        provider = %cand.provider.name,
        upstream_model = %cand.upstream_model_id,
        usage_source = %source,
        ended = "complete",
        image_output_tokens = usage.image_output,
        tokens = usage.total(),
        cost = %cost,
        "provider usage settled"
    );
}
