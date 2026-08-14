use crate::protocol::{gemini, openai};
use crate::transform::generate_content::openai_responses_to_gemini_generate_content::content::response_input_to_gemini_contents;
use crate::transform::{TransformContext, TransformError};

pub fn request(
    input: openai::CompactResponseRequestBody,
    _: &TransformContext,
) -> Result<gemini::GenerateContentRequest, TransformError> {
    Ok(crate::protocol::wire!(gemini::GenerateContentRequest {
        model: Some(model_string(input.model)),
        contents: response_input_to_gemini_contents(input.input),
        tools: Vec::new(),
        tool_config: None,
        safety_settings: Vec::new(),
        system_instruction: input.instructions.map(|text| crate::protocol::wire!(
            gemini::Content {
                parts: vec![crate::protocol::wire!(gemini::Part {
                    data: Some(gemini::PartData::Text { text }),
                    ..Default::default()
                })],
                role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
                extra: Default::default(),
            }
        )),
        generation_config: None,
        cached_content: input.prompt_cache_key,
        service_tier: input.service_tier.map(compact_service_tier_to_gemini),
        store: None,
        extra: Default::default(),
    }))
}

fn model_string(model: openai::OpenAiModelId) -> String {
    match model {
        openai::OpenAiModelId::Unknown(model) => model,
        known => serde_json::to_value(known)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown".to_owned()),
    }
}

fn compact_service_tier_to_gemini(tier: openai::CompactServiceTier) -> gemini::ServiceTier {
    let tier = match tier {
        openai::CompactServiceTier::Auto => gemini::ServiceTierKnown::Unspecified,
        openai::CompactServiceTier::Default => gemini::ServiceTierKnown::Standard,
        openai::CompactServiceTier::Fast | openai::CompactServiceTier::Priority => {
            gemini::ServiceTierKnown::Priority
        }
        openai::CompactServiceTier::Flex => gemini::ServiceTierKnown::Flex,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    gemini::ServiceTier::Known(tier)
}
