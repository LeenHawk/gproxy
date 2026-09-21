//! Token counts plus the price OpenRouter charged for the exchange.
//!
//! OpenRouter is a reseller: when the request asked for usage accounting the
//! reply carries `usage.cost`, the amount in USD credits actually debited,
//! and `usage.cost_details.*` breaking it down. That number is the truth for
//! this exchange, so it is recorded as the `upstream_cost_usd` metric and the
//! exchange is marked `upstream_priced`. Core prices from the store's rate
//! rows and knows nothing about this key by name, so an operator who wants
//! the upstream's own number to be the bill writes one rate row for
//! `upstream_cost_usd` at 1 USD per unit and no token rows; see the README.

use super::OpenRouter;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::ability::decimal;
use crate::channels::shared::compatible::usage::{self, u64_at};
use serde_json::Value;

/// The amount the upstream says it charged, in USD.
pub const UPSTREAM_COST_METRIC: &str = "upstream_cost_usd";
/// Set when the upstream priced the exchange itself.
pub const UPSTREAM_PRICED_DIMENSION: &str = "upstream_priced";

/// OpenRouter's own fields, off the usage object and the response root.
fn enrich(root: &Value, value: &Value, into: &mut NormalizedUsage) {
    if let Some(cost) = value.get("cost").and_then(decimal) {
        into.metrics.insert(UPSTREAM_COST_METRIC.into(), cost);
        into.dimensions
            .insert(UPSTREAM_PRICED_DIMENSION.into(), "true".into());
    }
    if let Some(details) = value.get("cost_details").and_then(Value::as_object) {
        for (name, amount) in details {
            if let Some(amount) = decimal(amount) {
                into.metrics.insert(format!("{name}_usd"), amount);
            }
        }
    }
    if let Some(tokens) = u64_at(value, "/prompt_tokens_details/video_tokens") {
        into.metrics
            .insert("video_input_tokens".into(), tokens.into());
    }
    // A bring-your-own-key exchange is billed by the underlying provider, so
    // it prices differently even at the same model and token counts.
    if let Some(byok) = value
        .get("is_byok")
        .or_else(|| root.pointer("/openrouter_metadata/is_byok"))
        .and_then(Value::as_bool)
    {
        into.dimensions.insert("is_byok".into(), byok.to_string());
    }
}

impl UsageExtractor for OpenRouter {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage::from_body(
            ctx.operation.dialect,
            ctx.response.body,
            enrich,
        ))
    }
}

impl UsageStream for OpenRouter {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(usage::observer(
            context.operation.dialect,
            context.transport,
            enrich,
        ))
    }
}
