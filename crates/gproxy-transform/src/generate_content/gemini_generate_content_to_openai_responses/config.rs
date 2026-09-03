use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(in crate::generate_content) fn gemini_reasoning(
    thinking: Option<&gemini::ThinkingConfig>,
) -> Option<openai::ReasoningEffort> {
    let thinking = thinking?;
    if thinking.include_thoughts == Some(false) {
        return Some(openai::ReasoningEffort::None);
    }
    match thinking.thinking_level.as_ref() {
        Some(gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal)) => {
            Some(openai::ReasoningEffort::Minimal)
        }
        Some(gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low)) => {
            Some(openai::ReasoningEffort::Low)
        }
        Some(gemini::ThinkingLevel::Known(
            gemini::ThinkingLevelKnown::Medium
            | gemini::ThinkingLevelKnown::ThinkingLevelUnspecified,
        )) => Some(openai::ReasoningEffort::Medium),
        Some(gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High)) => {
            Some(openai::ReasoningEffort::High)
        }
        Some(gemini::ThinkingLevel::Unknown(_)) => None,
        None if thinking.thinking_budget == Some(0) => Some(openai::ReasoningEffort::None),
        None => None,
        Some(_) => None,
    }
}

pub(in crate::generate_content) fn openai_reasoning(
    effort: Option<openai::ReasoningEffort>,
) -> Option<gemini::ThinkingConfig> {
    let effort = effort?;
    let (include_thoughts, thinking_level) = match effort {
        openai::ReasoningEffort::None => (Some(false), None),
        openai::ReasoningEffort::Minimal => (Some(true), Some(gemini::ThinkingLevelKnown::Minimal)),
        openai::ReasoningEffort::Low => (Some(true), Some(gemini::ThinkingLevelKnown::Low)),
        openai::ReasoningEffort::Medium => (Some(true), Some(gemini::ThinkingLevelKnown::Medium)),
        openai::ReasoningEffort::High
        | openai::ReasoningEffort::XHigh
        | openai::ReasoningEffort::Max => (Some(true), Some(gemini::ThinkingLevelKnown::High)),
        openai::ReasoningEffort::Unknown(_) => return None,
    };
    Some(crate::wire!(gemini::ThinkingConfig {
        include_thoughts,
        thinking_budget: None,
        thinking_level: thinking_level.map(gemini::ThinkingLevel::Known),
        rest: Default::default(),
    }))
}

pub(in crate::generate_content) fn gemini_service_tier(
    tier: Option<gemini::ServiceTier>,
) -> Option<openai::ServiceTier> {
    Some(match tier? {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Flex) => openai::ServiceTier::Flex,
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority) => {
            openai::ServiceTier::Priority
        }
        gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Standard | gemini::ServiceTierKnown::Unspecified,
        ) => openai::ServiceTier::Default,
        gemini::ServiceTier::Unknown(_) => return None,
        _ => openai::ServiceTier::Default,
    })
}

pub(in crate::generate_content) fn openai_service_tier(
    tier: Option<openai::ServiceTier>,
) -> Option<gemini::ServiceTier> {
    Some(gemini::ServiceTier::Known(match tier? {
        openai::ServiceTier::Auto => gemini::ServiceTierKnown::Unspecified,
        openai::ServiceTier::Flex => gemini::ServiceTierKnown::Flex,
        openai::ServiceTier::Fast
        | openai::ServiceTier::Priority
        | openai::ServiceTier::Ultrafast => gemini::ServiceTierKnown::Priority,
        openai::ServiceTier::Default
        | openai::ServiceTier::Scale
        | openai::ServiceTier::OnDemand => gemini::ServiceTierKnown::Standard,
        openai::ServiceTier::Unknown(_) => return None,
    }))
}

pub(in crate::generate_content) fn model_string(
    model: openai::OpenAiModelId,
) -> Result<String, TransformError> {
    serde_json::to_value(model)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("OpenAI model", "expected a string"))
}
