use crate::protocol::{claude, openai};

use super::super::scalar::{u32_to_u64, u64_to_u32};

pub(in crate::transform::generate_content) fn completion_usage_to_claude(
    usage: Option<openai::CompletionUsage>,
) -> claude::Usage {
    let Some(usage) = usage else {
        return empty_claude_usage();
    };
    claude::Usage {
        input_tokens: Some(u32_to_u64(usage.prompt_tokens)),
        output_tokens: Some(u32_to_u64(usage.completion_tokens)),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(u32_to_u64),
        cache_creation: None,
        output_tokens_details: usage.completion_tokens_details.and_then(|details| {
            details
                .reasoning_tokens
                .map(|tokens| claude::OutputTokensDetails {
                    thinking_tokens: u32_to_u64(tokens),
                    extra: Default::default(),
                })
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    }
}

pub(in crate::transform::generate_content) fn empty_claude_usage() -> claude::Usage {
    claude::Usage {
        input_tokens: Some(0),
        output_tokens: Some(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    }
}

pub(in crate::transform::generate_content) fn claude_usage_to_completion(
    usage: claude::Usage,
) -> openai::CompletionUsage {
    let prompt_tokens = usage.input_tokens.map(u64_to_u32).unwrap_or_default();
    let completion_tokens = usage.output_tokens.map(u64_to_u32).unwrap_or_default();
    let cached_tokens = usage.cache_read_input_tokens.map(u64_to_u32);
    let reasoning_tokens = usage
        .output_tokens_details
        .map(|details| u64_to_u32(details.thinking_tokens));

    openai::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens: prompt_tokens.saturating_add(completion_tokens),
        completion_tokens_details: reasoning_tokens.map(|reasoning_tokens| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(reasoning_tokens),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            }
        }),
        prompt_tokens_details: cached_tokens.map(|cached_tokens| openai::PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: Some(cached_tokens),
            extra: Default::default(),
        }),
        extra: Default::default(),
    }
}
