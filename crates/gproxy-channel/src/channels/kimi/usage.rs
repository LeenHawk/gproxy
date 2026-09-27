//! Kimi keeps its cache-hit count under a name of its own.

use crate::channel::{NormalizedUsage, UsageExtras, UsageSource, usage_object};
use serde_json::Value;

impl UsageExtras for super::Kimi {
    /// Chat Completions counts cache hits inside `prompt_tokens`, and
    /// Moonshot reports the hit count as `usage.cached_tokens` rather than in
    /// `prompt_tokens_details`. The standard reading therefore left the whole
    /// prompt in `input_tokens`; split it here. A reply that also carries the
    /// standard field has already been split and is left alone.
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        let Some(value) = usage_object(source.root) else {
            return;
        };
        if value.pointer("/prompt_tokens_details/cached_tokens").is_some() {
            return;
        }
        let Some(cached) = value.get("cached_tokens").and_then(Value::as_u64) else {
            return;
        };
        usage.tokens.cached_input_tokens = Some(cached);
        if let Some(input) = usage.tokens.input_tokens {
            usage.tokens.input_tokens = Some(input.saturating_sub(cached));
        }
    }
}
