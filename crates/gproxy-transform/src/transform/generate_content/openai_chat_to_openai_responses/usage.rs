use crate::protocol::openai;

pub(super) fn chat_usage_to_response(
    usage: Option<openai::CompletionUsage>,
) -> Option<openai::ResponseUsage> {
    let usage = usage?;
    let (cached_tokens, cache_write_tokens) = usage
        .prompt_tokens_details
        .map(|details| (details.cached_tokens, details.cache_write_tokens))
        .unwrap_or_default();
    let reasoning_tokens = usage
        .completion_tokens_details
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or_default();

    Some(openai::ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: (cached_tokens.is_some() || cache_write_tokens.is_some()).then(
            || openai::ResponseInputTokensDetails {
                cache_write_tokens: cache_write_tokens.unwrap_or_default(),
                cached_tokens: cached_tokens.unwrap_or_default(),
                extra: Default::default(),
            },
        ),
        output_tokens_details: openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        },
        extra: Default::default(),
    })
}
