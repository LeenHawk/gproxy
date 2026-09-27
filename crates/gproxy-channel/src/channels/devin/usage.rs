//! What the upstream reports beside the standard Chat Completions usage.
//!
//! The counters are the upstream's own, read from the `#7` metadata
//! sub-message of the terminal response frame and carried through
//! `stream.rs` into the Chat Completions usage object the channel hands
//! out, which the host reads the standard way. Two of them have no standard
//! field.
//!
//! Cache creation has no TTL on this wire, so it cannot claim one of the
//! `cache_creation_*` fields and travels as a named metric.
//!
//! `#7.9 actual_model_uid` — the concrete model behind a router or a family
//! alias — travels as a usage *dimension*. It is not echoed as the response
//! `model`, which clients compare against what they asked for, but it is the
//! only signal of what actually ran and therefore of what is actually being
//! billed, so pricing and metering see it.

use rust_decimal::Decimal;
use serde_json::Value;

use super::{Devin, stream::ACTUAL_MODEL_KEY};
use crate::channel::{NormalizedUsage, UsageExtras, UsageSource, usage_object};

/// The key a cache-creation count is reported under.
pub const CACHE_CREATION_METRIC: &str = "cache_creation_tokens";

impl UsageExtras for Devin {
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        let Some(value) = usage_object(source.root) else {
            return;
        };
        if let Some(created) = value
            .get("cache_creation_input_tokens")
            .and_then(Value::as_u64)
        {
            usage
                .metrics
                .insert(CACHE_CREATION_METRIC.into(), Decimal::from(created));
        }
        if let Some(actual) = value.get(ACTUAL_MODEL_KEY).and_then(Value::as_str) {
            usage
                .dimensions
                .insert(ACTUAL_MODEL_KEY.into(), actual.to_owned());
        }
    }
}
