//! DeepSeek reports its cache hits under a name of its own.

use super::DeepSeek;
use crate::channel::{NormalizedUsage, UsageExtras, UsageSource, usage_object};
use serde_json::Value;

impl UsageExtras for DeepSeek {
    /// `prompt_tokens` counts cache hits and misses together, as Chat
    /// Completions does, but the hit count lives at
    /// `usage.prompt_cache_hit_tokens` rather than in
    /// `prompt_tokens_details`. The standard reading therefore left the whole
    /// prompt in `input_tokens`; split it here.
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        let Some(hit) = usage_object(source.root)
            .and_then(|value| value.get("prompt_cache_hit_tokens"))
            .and_then(Value::as_u64)
        else {
            return;
        };
        usage.tokens.cached_input_tokens = Some(hit);
        if let Some(input) = usage.tokens.input_tokens {
            usage.tokens.input_tokens = Some(input.saturating_sub(hit));
        }
    }
}
