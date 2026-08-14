use crate::protocol::openai;
use crate::transform::generate_content::openai_responses_to_openai_chat::content::response_input_to_chat_messages;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::CompactResponseRequestBody,
    _: &TransformContext,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let mut messages = Vec::new();
    if let Some(instructions) = input.instructions {
        messages.push(openai::ChatCompletionMessageParam::Developer {
            content: openai::ChatTextContent::Text(instructions),
            name: None,
            extra: Default::default(),
        });
    }
    messages.extend(response_input_to_chat_messages(input.input));

    Ok(crate::protocol::wire!(openai::ChatCompletionRequest {
        messages,
        model: input.model,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier.map(compact_service_tier_to_chat),
        audio: None,
        frequency_penalty: None,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: None,
        max_completion_tokens: None,
        max_tokens: None,
        metadata: None,
        modalities: None,
        moderation: None,
        n: None,
        parallel_tool_calls: None,
        prediction: None,
        presence_penalty: None,
        reasoning_effort: None,
        response_format: None,
        safety_identifier: None,
        seed: None,
        stop: None,
        store: None,
        stream: None,
        stream_options: None,
        temperature: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        user: None,
        verbosity: None,
        web_search_options: None,
        extra: Default::default(),
    }))
}

fn compact_service_tier_to_chat(tier: openai::CompactServiceTier) -> openai::ServiceTier {
    match tier {
        openai::CompactServiceTier::Auto => openai::ServiceTier::Auto,
        openai::CompactServiceTier::Default => openai::ServiceTier::Default,
        openai::CompactServiceTier::Fast => openai::ServiceTier::Fast,
        openai::CompactServiceTier::Flex => openai::ServiceTier::Flex,
        openai::CompactServiceTier::Priority => openai::ServiceTier::Priority,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}
