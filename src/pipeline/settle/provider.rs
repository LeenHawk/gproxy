//! §17 settlement for the provider-shaped billable ops that are NOT
//! content-generation: embeddings and image generation. These are always
//! non-streaming single-JSON responses, so they settle inline from the buffered
//! body (no counting ladder, no stream guard). The content-generation settle
//! path ([`super::SettleCtx`]) is untouched.
//!
//! Pricing: embeddings reuse the per-million-token `input` rate (the response's
//! `usage.prompt_tokens`); images are billed per image at the flat `image` rate
//! (counted from the response `data` array).

use bytes::Bytes;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::app::AppState;
use crate::billing::{self, UsageRecord, price};
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::protocol::{Operation, Provider as Family};
use crate::usage::{Ended, NormalizedUsage, UsageSource, extract};
use crate::util::time::unix_now;

/// Settle a successful embedding / image response. No-op for any other
/// operation (the caller invokes this for every successful buffered response;
/// content-generation, models and count ops return early here).
pub(crate) async fn settle(state: &AppState, ctx: &RequestCtx, cand: &Candidate, body: &Bytes) {
    let Some(op) = ctx.op else { return };
    let is_embedding = matches!(op.operation, Operation::CreateEmbedding);
    let is_image = matches!(op.operation, Operation::CreateImage | Operation::EditImage);
    if !is_embedding && !is_image {
        return;
    }

    // Resolve pricing + quota scopes under a scoped snapshot guard (the await
    // below never touches the snapshot). For images the raw `pricing_json` is
    // cloned out for tiered (size/quality) lookup.
    let identity = ctx.identity.as_deref();
    let (pricing, image_pricing, quota_scopes) = {
        let cp = state.cp();
        let pricing =
            billing::pending::model_pricing(&cp, cand.provider.id, &cand.upstream_model_id);
        let image_pricing = is_image
            .then(|| {
                cp.models_by_provider
                    .get(&cand.provider.id)
                    .and_then(|ms| ms.iter().find(|m| m.model_id == cand.upstream_model_id))
                    .and_then(|m| m.pricing_json.clone())
            })
            .flatten();
        let scopes = identity
            .map(|i| crate::pipeline::authz::quota_scopes(&cp, i))
            .unwrap_or_default();
        (pricing, image_pricing, scopes)
    };

    let parsed: Option<Value> = serde_json::from_slice(body).ok();
    let (usage, cost) = if is_embedding {
        let usage = parsed
            .as_ref()
            .and_then(|v| extract::from_response(Family::OpenAi, v))
            .unwrap_or_default();
        (usage, price::cost(&usage, &pricing))
    } else {
        // Images: bill per image in the response `data` array, at the rate for
        // the requested size/quality (read from the inbound request body).
        let count = parsed
            .as_ref()
            .and_then(|v| v.get("data"))
            .and_then(Value::as_array)
            .map(|a| a.len() as u64)
            .unwrap_or(0);
        let req: Option<Value> = serde_json::from_slice(&ctx.body).ok();
        let field = |k: &str| {
            req.as_ref()
                .and_then(|r| r.get(k))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let rate = price::image_rate(
            image_pricing.as_ref(),
            field("size").as_deref(),
            field("quality").as_deref(),
        );
        (NormalizedUsage::default(), Decimal::from(count) * rate)
    };

    let operation = super::enum_str(&op.operation);
    let kind = super::enum_str(&op.kind);
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
        source: UsageSource::Upstream,
        ended: Ended::Complete,
    };
    // §8-E: `enable_usage` gates the usage row only — the reconcile below
    // (pending refund + quota cost) is billing correctness and always runs.
    if state.cp().log_settings.enable_usage
        && let Err(e) = billing::record_success(state.persistence.as_ref(), rec).await
    {
        tracing::warn!(request_id = %ctx.request_id, error = %e, "embedding/image settle write failed");
    }
    // §17 reconcile, symmetric with the content-generation path: refund the
    // pre-deducted pending (charged in `execute`), then persist the actual cost
    // into each quota row. `refund` is a no-op when nothing was pre-deducted.
    billing::pending::refund(state.cache.as_ref(), &quota_scopes, ctx.pending_micros).await;
    if cost > Decimal::ZERO {
        for (scope, scope_id) in &quota_scopes {
            if let Err(e) = state
                .persistence
                .add_quota_cost(*scope, *scope_id, cost)
                .await
            {
                tracing::warn!(request_id = %ctx.request_id, error = %e, "embedding/image quota write failed");
            }
        }
    }
}
