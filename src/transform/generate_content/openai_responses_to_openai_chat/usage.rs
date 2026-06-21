use crate::protocol::openai;

pub(super) fn response_usage_to_chat(
    usage: Option<openai::ResponseUsage>,
) -> Option<openai::CompletionUsage> {
    let usage = usage?;
    let cached_tokens = usage
        .input_tokens_details
        .map(|details| details.cached_tokens);
    let reasoning_tokens = usage.output_tokens_details.reasoning_tokens;

    Some(openai::CompletionUsage {
        completion_tokens: usage.output_tokens,
        prompt_tokens: usage.input_tokens,
        total_tokens: usage.total_tokens,
        completion_tokens_details: (reasoning_tokens > 0).then_some(
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(reasoning_tokens),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            },
        ),
        prompt_tokens_details: cached_tokens.map(|cached_tokens| openai::PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: Some(cached_tokens),
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}
