use gproxy_protocol::{claude, openai};

pub(crate) fn claude_to_chat(usage: claude::Usage) -> Option<openai::CompletionUsage> {
    let cached = usage.cache_read_input_tokens;
    let cache_write = usage.cache_creation_total();
    let input = add_present(usage.input_tokens?, [cached, cache_write]);
    let output = usage.output_tokens?;
    Some(openai::CompletionUsage {
        completion_tokens: clamp(output),
        prompt_tokens: clamp(input),
        total_tokens: clamp(input.saturating_add(output)),
        completion_tokens_details: usage.output_tokens_details.map(|details| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: details.thinking_tokens.map(clamp),
                rejected_prediction_tokens: None,
                rest: Default::default(),
            }
        }),
        prompt_tokens_details: cache_details_to_chat(cached, cache_write),
        rest: Default::default(),
    })
}

pub(crate) fn chat_to_claude(usage: Option<openai::CompletionUsage>) -> Option<claude::Usage> {
    let usage = usage?;
    let cached = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens);
    let cache_write = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cache_write_tokens);
    Some(claude::Usage {
        input_tokens: Some(subtract_present(
            u64::from(usage.prompt_tokens),
            [cached, cache_write],
        )),
        output_tokens: Some(u64::from(usage.completion_tokens)),
        cache_creation_input_tokens: cache_write.map(u64::from),
        cache_read_input_tokens: cached.map(u64::from),
        cache_creation: None,
        output_tokens_details: usage.completion_tokens_details.map(|details| {
            claude::OutputTokensDetails {
                thinking_tokens: details.reasoning_tokens.map(u64::from),
                rest: Default::default(),
            }
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        rest: Default::default(),
    })
}

pub(crate) fn claude_to_responses(usage: claude::Usage) -> Option<openai::ResponseUsage> {
    let cached = usage.cache_read_input_tokens;
    let cache_write = usage.cache_creation_total();
    let input = add_present(usage.input_tokens?, [cached, cache_write]);
    let output = usage.output_tokens?;
    Some(openai::ResponseUsage {
        input_tokens: clamp(input),
        output_tokens: clamp(output),
        total_tokens: clamp(input.saturating_add(output)),
        input_tokens_details: cache_details_to_responses(cached, cache_write),
        output_tokens_details: usage.output_tokens_details.map(|details| {
            openai::ResponseOutputTokensDetails {
                reasoning_tokens: details.thinking_tokens.map(clamp),
                rest: Default::default(),
            }
        }),
        rest: Default::default(),
    })
}

pub(crate) fn responses_to_claude(usage: Option<openai::ResponseUsage>) -> Option<claude::Usage> {
    let usage = usage?;
    let cached = usage
        .input_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens);
    let cache_write = usage
        .input_tokens_details
        .as_ref()
        .and_then(|details| details.cache_write_tokens);
    Some(claude::Usage {
        input_tokens: Some(subtract_present(
            u64::from(usage.input_tokens),
            [cached, cache_write],
        )),
        output_tokens: Some(u64::from(usage.output_tokens)),
        cache_creation_input_tokens: cache_write.map(u64::from),
        cache_read_input_tokens: cached.map(u64::from),
        cache_creation: None,
        output_tokens_details: usage.output_tokens_details.map(|details| {
            claude::OutputTokensDetails {
                thinking_tokens: details.reasoning_tokens.map(u64::from),
                rest: Default::default(),
            }
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        rest: Default::default(),
    })
}

pub(crate) fn responses_to_chat(usage: openai::ResponseUsage) -> openai::CompletionUsage {
    openai::CompletionUsage {
        completion_tokens: usage.output_tokens,
        prompt_tokens: usage.input_tokens,
        total_tokens: usage.total_tokens,
        completion_tokens_details: usage.output_tokens_details.map(|details| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: details.reasoning_tokens,
                rejected_prediction_tokens: None,
                rest: Default::default(),
            }
        }),
        prompt_tokens_details: usage.input_tokens_details.map(|details| {
            openai::PromptTokensDetails {
                audio_tokens: None,
                cache_write_tokens: details.cache_write_tokens,
                cached_tokens: details.cached_tokens,
                rest: Default::default(),
            }
        }),
        rest: Default::default(),
    }
}

pub(crate) fn chat_to_responses(usage: openai::CompletionUsage) -> openai::ResponseUsage {
    openai::ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: usage.prompt_tokens_details.map(|details| {
            openai::ResponseInputTokensDetails {
                cache_write_tokens: details.cache_write_tokens,
                cached_tokens: details.cached_tokens,
                rest: Default::default(),
            }
        }),
        output_tokens_details: usage.completion_tokens_details.map(|details| {
            openai::ResponseOutputTokensDetails {
                reasoning_tokens: details.reasoning_tokens,
                rest: Default::default(),
            }
        }),
        rest: Default::default(),
    }
}

fn cache_details_to_chat(
    cached: Option<u64>,
    cache_write: Option<u64>,
) -> Option<openai::PromptTokensDetails> {
    (cached.is_some() || cache_write.is_some()).then(|| openai::PromptTokensDetails {
        audio_tokens: None,
        cache_write_tokens: cache_write.map(clamp),
        cached_tokens: cached.map(clamp),
        rest: Default::default(),
    })
}

fn cache_details_to_responses(
    cached: Option<u64>,
    cache_write: Option<u64>,
) -> Option<openai::ResponseInputTokensDetails> {
    (cached.is_some() || cache_write.is_some()).then(|| openai::ResponseInputTokensDetails {
        cache_write_tokens: cache_write.map(clamp),
        cached_tokens: cached.map(clamp),
        rest: Default::default(),
    })
}

fn add_present<const N: usize>(mut value: u64, details: [Option<u64>; N]) -> u64 {
    for detail in details.into_iter().flatten() {
        value = value.saturating_add(detail);
    }
    value
}

fn subtract_present<const N: usize>(mut value: u64, details: [Option<u32>; N]) -> u64 {
    for detail in details.into_iter().flatten() {
        value = value.saturating_sub(u64::from(detail));
    }
    value
}

fn clamp(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_usage_and_optional_details_stay_absent() {
        let usage = claude::Usage {
            input_tokens: Some(10),
            output_tokens: Some(3),
            cache_creation_input_tokens: None,
            cache_read_input_tokens: Some(2),
            cache_creation: None,
            output_tokens_details: None,
            server_tool_use: None,
            iterations: None,
            inference_geo: None,
            service_tier: None,
            speed: None,
            rest: Default::default(),
        };
        let response = claude_to_responses(usage.clone()).unwrap();
        let details = response.input_tokens_details.as_ref().unwrap();
        assert_eq!(details.cached_tokens, Some(2));
        assert_eq!(details.cache_write_tokens, None);
        assert_eq!(response.output_tokens_details, None);

        let chat = claude_to_chat(usage).unwrap();
        let details = chat.prompt_tokens_details.as_ref().unwrap();
        assert_eq!(details.cached_tokens, Some(2));
        assert_eq!(details.cache_write_tokens, None);
        assert_eq!(chat.completion_tokens_details, None);

        assert_eq!(chat_to_claude(None), None);
        assert_eq!(responses_to_claude(None), None);

        let incomplete = claude::Usage {
            input_tokens: Some(10),
            output_tokens: None,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
            cache_creation: None,
            output_tokens_details: None,
            server_tool_use: None,
            iterations: None,
            inference_geo: None,
            service_tier: None,
            speed: None,
            rest: Default::default(),
        };
        assert_eq!(claude_to_chat(incomplete.clone()), None);
        assert_eq!(claude_to_responses(incomplete), None);
    }
}
