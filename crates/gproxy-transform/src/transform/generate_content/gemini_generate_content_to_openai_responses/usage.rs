use crate::protocol::{gemini, openai};

pub(super) fn gemini_usage_to_response(usage: gemini::UsageMetadata) -> openai::ResponseUsage {
    let input_tokens = usage.prompt_token_count.map(i32_to_u32).unwrap_or_default();
    let cached_tokens = usage
        .cached_content_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let reasoning_tokens = usage
        .thoughts_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let output_tokens = usage
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_default()
        .saturating_add(reasoning_tokens);
    let total_tokens = usage
        .total_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));

    crate::protocol::wire!(openai::ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        input_tokens_details: (cached_tokens > 0).then(|| crate::protocol::wire!(
            openai::ResponseInputTokensDetails {
                cache_write_tokens: 0,
                cached_tokens,
                extra: Default::default(),
            }
        )),
        output_tokens_details: crate::protocol::wire!(openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}
