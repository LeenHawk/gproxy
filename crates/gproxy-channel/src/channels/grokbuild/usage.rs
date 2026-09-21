//! Per-call metering.
//!
//! The chat proxy answers OpenAI Chat Completions and Responses verbatim and
//! meters them the way xAI does, so the reading is
//! `shared::compatible::usage`'s with a hook for xAI's own fields: the
//! `cost_in_usd_ticks` unit, the image and web-search breakdowns, and a media
//! job's stated dollars. v3 delegated to the `xai` channel for exactly this;
//! v4 keeps the *policy* — which fields to read and what to call them — in
//! each channel, because a shared module only executes what a channel asks
//! for, so the hook is stated here rather than borrowed from `xai`.

use super::GrokBuild;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::ability::decimal;
use crate::channels::shared::compatible::usage::{self, u64_at};
use serde_json::Value;

/// xAI's own metering unit. It is not a currency: an operator prices it with
/// a rate row of its own rather than treating it as dollars.
pub const COST_TICKS_METRIC: &str = "cost_in_usd_ticks";
/// A media job states its price in dollars, so that exchange is priced.
pub const UPSTREAM_COST_METRIC: &str = "upstream_cost_usd";
/// Set when the upstream priced the exchange itself.
pub const UPSTREAM_PRICED_DIMENSION: &str = "upstream_priced";

fn enrich(root: &Value, value: &Value, into: &mut NormalizedUsage) {
    if let Some(ticks) = value.get("cost_in_usd_ticks").and_then(decimal) {
        into.metrics.insert(COST_TICKS_METRIC.into(), ticks);
    }
    if let Some(tokens) = u64_at(value, "/input_tokens_details/image_tokens") {
        into.metrics
            .insert("image_input_tokens".into(), tokens.into());
    }
    if let Some(searches) = u64_at(value, "/server_side_tool_usage_details/web_search_requests") {
        into.metrics.insert("web_searches".into(), searches.into());
    }
    // Only the field that names its unit is read; a bare `cost` elsewhere
    // states no currency.
    let Some(cost) = root.get("cost_usd").and_then(decimal) else {
        return;
    };
    into.metrics.insert(UPSTREAM_COST_METRIC.into(), cost);
    into.dimensions
        .insert(UPSTREAM_PRICED_DIMENSION.into(), "true".into());
    if let Some(seconds) = root
        .get("duration")
        .or_else(|| root.get("seconds"))
        .and_then(decimal)
    {
        into.metrics.insert("video_seconds".into(), seconds);
    }
}

impl UsageExtractor for GrokBuild {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage::from_body(
            context.operation.dialect,
            context.response.body,
            enrich,
        ))
    }
}

impl UsageStream for GrokBuild {
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
