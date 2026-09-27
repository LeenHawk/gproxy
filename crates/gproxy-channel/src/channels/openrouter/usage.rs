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
use crate::channel::{NormalizedUsage, UsageAttempt, UsageExtras, UsageSource, usage_object};
use crate::channels::shared::compatible::ability::decimal;
use serde_json::Value;

/// The amount the upstream says it charged, in USD.
pub const UPSTREAM_COST_METRIC: &str = "upstream_cost_usd";
/// Set when the upstream priced the exchange itself.
pub const UPSTREAM_PRICED_DIMENSION: &str = "upstream_priced";

impl UsageExtras for OpenRouter {
    /// OpenRouter's own fields, off the usage object and the response root.
    /// The serving model also becomes the one attempt, so pricing sees the
    /// model that actually ran rather than the router alias that was asked
    /// for.
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        let root = source.root;
        if let Some(model) = root
            .get("model")
            .or_else(|| root.pointer("/message/model"))
            .or_else(|| root.pointer("/response/model"))
            .and_then(Value::as_str)
        {
            usage
                .dimensions
                .insert("serving_model".into(), model.to_owned());
        }
        let value = usage_object(root).unwrap_or(&Value::Null);
        if let Some(cost) = value.get("cost").and_then(decimal) {
            usage.metrics.insert(UPSTREAM_COST_METRIC.into(), cost);
            usage
                .dimensions
                .insert(UPSTREAM_PRICED_DIMENSION.into(), "true".into());
        }
        if let Some(details) = value.get("cost_details").and_then(Value::as_object) {
            for (name, amount) in details {
                if let Some(amount) = decimal(amount) {
                    usage.metrics.insert(format!("{name}_usd"), amount);
                }
            }
        }
        if let Some(tokens) = value
            .pointer("/prompt_tokens_details/video_tokens")
            .and_then(Value::as_u64)
        {
            usage
                .metrics
                .insert("video_input_tokens".into(), tokens.into());
        }
        // A bring-your-own-key exchange is billed by the underlying provider,
        // so it prices differently even at the same model and token counts.
        if let Some(byok) = value
            .get("is_byok")
            .or_else(|| root.pointer("/openrouter_metadata/is_byok"))
            .and_then(Value::as_bool)
        {
            usage.dimensions.insert("is_byok".into(), byok.to_string());
        }
        if let Some(model) = usage.dimensions.get("serving_model").cloned() {
            usage.attempts = vec![UsageAttempt {
                model,
                usage: Box::new(usage.clone()),
                billable: None,
                started_at_ms: None,
            }];
        }
    }
}
