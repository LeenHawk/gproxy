//! DeepSeek reports its cache hits under a name of its own.

use super::DeepSeek;
use crate::channel::{
    ChannelError, NormalizedUsage, UsageContext, UsageExtractor, UsageObserver, UsageStream,
    UsageStreamContext,
};
use crate::channels::shared::compatible::usage::{self, u64_at};
use serde_json::Value;

/// `prompt_tokens` counts cache hits and misses together, as Chat
/// Completions does, but the hit count lives at `usage.prompt_cache_hit_tokens`
/// rather than in `prompt_tokens_details`. The shared reader therefore left
/// the whole prompt in `input_tokens`; split it here.
fn enrich(_: &Value, value: &Value, into: &mut NormalizedUsage) {
    let Some(hit) = u64_at(value, "/prompt_cache_hit_tokens") else {
        return;
    };
    into.tokens.cached_input_tokens = Some(hit);
    if let Some(input) = into.tokens.input_tokens {
        into.tokens.input_tokens = Some(input.saturating_sub(hit));
    }
}

impl UsageExtractor for DeepSeek {
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

impl UsageStream for DeepSeek {
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
