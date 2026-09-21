//! Kimi keeps its cache-hit count under a name of its own.

use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::usage::{self, u64_at};
use serde_json::Value;

/// Chat Completions counts cache hits inside `prompt_tokens`, and Moonshot
/// reports the hit count as `usage.cached_tokens` rather than in
/// `prompt_tokens_details`. The shared reader therefore left the whole prompt
/// in `input_tokens`; split it here. A reply that also carries the standard
/// field has already been split and is left alone.
fn enrich(_: &Value, value: &Value, into: &mut NormalizedUsage) {
    if u64_at(value, "/prompt_tokens_details/cached_tokens").is_some() {
        return;
    }
    let Some(cached) = u64_at(value, "/cached_tokens") else {
        return;
    };
    into.tokens.cached_input_tokens = Some(cached);
    if let Some(input) = into.tokens.input_tokens {
        into.tokens.input_tokens = Some(input.saturating_sub(cached));
    }
}

impl UsageExtractor for super::Kimi {
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

impl UsageStream for super::Kimi {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gproxy_protocol::Dialect;
    use serde_json::json;

    #[test]
    fn a_moonshot_cache_hit_leaves_the_input_count_exclusive() {
        let body = json!({"usage": {"prompt_tokens": 100, "completion_tokens": 3,
            "cached_tokens": 60}});
        let usage = usage::from_root(Dialect::OpenAiChat, &body, enrich).unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(40));
        assert_eq!(usage.tokens.cached_input_tokens, Some(60));
    }

    #[test]
    fn the_standard_field_wins_and_is_not_subtracted_twice() {
        let body = json!({"usage": {"prompt_tokens": 100, "completion_tokens": 3,
            "cached_tokens": 60, "prompt_tokens_details": {"cached_tokens": 60}}});
        let usage = usage::from_root(Dialect::OpenAiChat, &body, enrich).unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(40));
        assert_eq!(usage.tokens.cached_input_tokens, Some(60));
    }
}
