use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::CompactResponseRequestBody,
    _: &TransformContext,
) -> Result<openai::ResponseCreateRequest, TransformError> {
    Ok(openai::ResponseCreateRequest {
        input: input.input,
        instructions: input.instructions,
        model: Some(input.model),
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier.map(compact_service_tier_to_response),
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        moderation: None,
        parallel_tool_calls: None,
        prompt: None,
        reasoning: None,
        safety_identifier: None,
        store: None,
        stream: None,
        stream_options: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        user: None,
        extra: Default::default(),
    })
}

fn compact_service_tier_to_response(
    service_tier: openai::CompactServiceTier,
) -> openai::ServiceTier {
    match service_tier {
        openai::CompactServiceTier::Auto => openai::ServiceTier::Auto,
        openai::CompactServiceTier::Default => openai::ServiceTier::Default,
        openai::CompactServiceTier::Flex => openai::ServiceTier::Flex,
        openai::CompactServiceTier::Priority => openai::ServiceTier::Priority,
    }
}
