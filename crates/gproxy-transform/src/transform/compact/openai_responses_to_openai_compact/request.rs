use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::ResponseCreateRequest,
    _: &TransformContext,
) -> Result<openai::CompactResponseRequestBody, TransformError> {
    Ok(openai::CompactResponseRequestBody {
        input: input.input,
        instructions: input.instructions,
        model: input
            .model
            .unwrap_or_else(|| openai::OpenAiModelId::Unknown("unknown".to_owned())),
        previous_response_id: input.previous_response_id,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_retention: input.prompt_cache_retention,
        service_tier: input.service_tier.map(service_tier_to_compact),
        extra: Default::default(),
    })
}

fn service_tier_to_compact(service_tier: openai::ServiceTier) -> openai::CompactServiceTier {
    match service_tier {
        openai::ServiceTier::Auto => openai::CompactServiceTier::Auto,
        openai::ServiceTier::Default | openai::ServiceTier::OnDemand => {
            openai::CompactServiceTier::Default
        }
        openai::ServiceTier::Flex => openai::CompactServiceTier::Flex,
        openai::ServiceTier::Priority => openai::CompactServiceTier::Priority,
        // `scale` has no compact equivalent; fall back to auto.
        openai::ServiceTier::Scale => openai::CompactServiceTier::Auto,
    }
}
