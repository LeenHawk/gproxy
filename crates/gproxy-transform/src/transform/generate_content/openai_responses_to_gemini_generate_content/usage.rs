use crate::protocol::{gemini, openai};

pub(super) fn response_usage_to_gemini(
    usage: Option<openai::ResponseUsage>,
) -> Option<gemini::UsageMetadata> {
    let usage = usage?;
    let cached_tokens = usage
        .input_tokens_details
        .as_ref()
        .map(|details| details.cached_tokens);
    let reasoning_tokens = usage.output_tokens_details.reasoning_tokens;

    Some(crate::protocol::wire!(gemini::UsageMetadata {
        prompt_token_count: Some(u32_to_i32(usage.input_tokens)),
        cached_content_token_count: cached_tokens.map(u32_to_i32),
        candidates_token_count: Some(u32_to_i32(
            usage.output_tokens.saturating_sub(reasoning_tokens)
        )),
        tool_use_prompt_token_count: None,
        thoughts_token_count: (reasoning_tokens > 0).then_some(u32_to_i32(reasoning_tokens)),
        total_token_count: Some(u32_to_i32(usage.total_tokens)),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        extra: Default::default(),
    }))
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}
