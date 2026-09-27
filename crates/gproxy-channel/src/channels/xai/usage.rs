//! What xAI reports beside the standard usage: the cost ticks it meters
//! with, a web-search count under a name of its own, and a video job's own
//! price.

use super::Xai;
use crate::channel::{NormalizedUsage, UsageExtras, UsageSource, usage_object};
use crate::channels::shared::compatible::ability::decimal;
use serde_json::Value;

/// xAI's own metering unit. It is not a currency: an operator prices it with
/// a rate row of its own rather than treating it as dollars.
pub const COST_TICKS_METRIC: &str = "cost_in_usd_ticks";
/// A video job states its price in dollars, so that exchange is priced.
pub const UPSTREAM_COST_METRIC: &str = "upstream_cost_usd";
/// Set when the upstream priced the exchange itself.
pub const UPSTREAM_PRICED_DIMENSION: &str = "upstream_priced";

fn enrich(root: &Value, into: &mut NormalizedUsage) {
    if let Some(value) = usage_object(root) {
        usage_fields(value, into);
    }
    // A video job answers with its own price at the response root; only that
    // field is read, because a bare `cost` elsewhere has no stated unit.
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

/// The usage object's own fields: the cost ticks and the web searches
/// counted under a name of xAI's own. The image input tokens beside them are
/// standard Responses detail, already read by the standard reading.
fn usage_fields(value: &Value, into: &mut NormalizedUsage) {
    if let Some(ticks) = value.get("cost_in_usd_ticks").and_then(decimal) {
        into.metrics.insert(COST_TICKS_METRIC.into(), ticks);
    }
    if let Some(searches) = value
        .pointer("/server_side_tool_usage_details/web_search_requests")
        .and_then(Value::as_u64)
    {
        into.metrics.insert("web_searches".into(), searches.into());
    }
}

impl UsageExtras for Xai {
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        enrich(source.root, usage);
    }
}
