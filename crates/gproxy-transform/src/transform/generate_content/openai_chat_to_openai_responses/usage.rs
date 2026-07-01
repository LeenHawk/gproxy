use crate::protocol::openai;

pub(super) fn chat_usage_to_response(
    usage: Option<openai::CompletionUsage>,
) -> Option<openai::ResponseUsage> {
    let usage = usage?;
    let cached_tokens = usage
        .prompt_tokens_details
        .and_then(|details| details.cached_tokens);
    let reasoning_tokens = usage
        .completion_tokens_details
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or_default();

    Some(openai::ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: cached_tokens.map(|cached_tokens| {
            openai::ResponseInputTokensDetails {
                cached_tokens,
                extra: Default::default(),
            }
        }),
        output_tokens_details: openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        },
        extra: Default::default(),
    })
}
