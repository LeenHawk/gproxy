use gproxy_channel_api::UsageAttempt;
use rust_decimal::Decimal;

use crate::funnel::FunnelCtx;
use crate::usage::SettledAttempt;

pub(super) fn price(ctx: &FunnelCtx, attempts: Vec<UsageAttempt>) -> Vec<SettledAttempt> {
    attempts.into_iter().map(|attempt| {
        let model = if attempt.model.is_empty() { ctx.target.upstream_model.clone() }
            else { ctx.usage_channel.as_ref().map_or_else(|| attempt.model.clone(), |channel| channel.fallback_model(&ctx.target.upstream_model, &attempt.model)) };
        let pricing = ctx.pricing_control.as_ref().and_then(|control| {
            control.pricing(&ctx.target.provider, &model).or_else(|| control.pricing(&ctx.target.provider, &attempt.model))
        }).or_else(|| (model == ctx.target.upstream_model).then(|| ctx.pricing.clone()).flatten());
        let cost = if !attempt.billable {
            Decimal::ZERO
        } else if let Some(pricing) = pricing {
            pricing.for_request(&ctx.request_body).cost(&attempt.usage)
        } else {
            tracing::warn!(request_id = %ctx.request_id, model = %attempt.model, "fallback attempt pricing missing; settling at zero cost");
            Decimal::ZERO
        };
        SettledAttempt {
            upstream_model: model,
            usage: *attempt.usage,
            cost,
            billable: attempt.billable,
            source: if attempt.estimated { crate::usage::UsageSource::Estimated } else { crate::usage::UsageSource::Upstream },
            started_at_ms: attempt.started_at_ms.or(ctx.upstream_started_at_ms),
        }
    }).collect()
}
