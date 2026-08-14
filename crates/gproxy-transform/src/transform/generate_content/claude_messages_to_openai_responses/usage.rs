use crate::protocol::{claude, openai};

pub(super) fn claude_usage_to_response(usage: claude::Usage) -> openai::ResponseUsage {
    let cached_tokens = usage.cache_read_input_tokens.map(u64_to_u32);
    let cache_write_tokens = usage.cache_creation_total().map(u64_to_u32);
    let input_tokens = usage
        .input_tokens
        .map(u64_to_u32)
        .unwrap_or_default()
        .saturating_add(cached_tokens.unwrap_or_default())
        .saturating_add(cache_write_tokens.unwrap_or_default());
    let output_tokens = usage.output_tokens.map(u64_to_u32).unwrap_or_default();
    let reasoning_tokens = usage
        .output_tokens_details
        .map(|details| u64_to_u32(details.thinking_tokens))
        .unwrap_or_default();

    crate::protocol::wire!(openai::ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        input_tokens_details: (cached_tokens.is_some() || cache_write_tokens.is_some()).then(
            || crate::protocol::wire!(openai::ResponseInputTokensDetails {
                cache_write_tokens: cache_write_tokens.unwrap_or_default(),
                cached_tokens: cached_tokens.unwrap_or_default(),
                extra: Default::default(),
            }),
        ),
        output_tokens_details: crate::protocol::wire!(openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
