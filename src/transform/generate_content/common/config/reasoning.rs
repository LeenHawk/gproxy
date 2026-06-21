use crate::protocol::{claude, gemini, openai};

use super::super::scalar::{i32_to_u64, u64_to_i32};

pub(in crate::transform::generate_content) fn openai_reasoning_to_claude(
    effort: Option<openai::ReasoningEffort>,
) -> Option<claude::ThinkingConfig> {
    match effort? {
        openai::ReasoningEffort::None => {
            Some(claude::ThinkingConfig::Disabled(claude::ThinkingDisabled {
                type_: claude::ThinkingDisabledType::Disabled,
                extra: Default::default(),
            }))
        }
        _ => Some(claude::ThinkingConfig::Adaptive(claude::ThinkingAdaptive {
            type_: claude::ThinkingAdaptiveType::Adaptive,
            display: None,
            extra: Default::default(),
        })),
    }
}

pub(in crate::transform::generate_content) fn claude_thinking_to_openai(
    thinking: Option<claude::ThinkingConfig>,
) -> Option<openai::ReasoningEffort> {
    match thinking? {
        claude::ThinkingConfig::Disabled(_) => Some(openai::ReasoningEffort::None),
        claude::ThinkingConfig::Enabled(_) | claude::ThinkingConfig::Adaptive(_) => {
            Some(openai::ReasoningEffort::Medium)
        }
        claude::ThinkingConfig::Unknown(_) => None,
    }
}

pub(in crate::transform::generate_content) fn openai_reasoning_to_gemini(
    effort: Option<openai::ReasoningEffort>,
) -> Option<gemini::ThinkingConfig> {
    Some(gemini::ThinkingConfig {
        include_thoughts: None,
        thinking_budget: None,
        thinking_level: Some(match effort? {
            openai::ReasoningEffort::None | openai::ReasoningEffort::Minimal => {
                gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal)
            }
            openai::ReasoningEffort::Low => {
                gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low)
            }
            openai::ReasoningEffort::Medium => {
                gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Medium)
            }
            openai::ReasoningEffort::High | openai::ReasoningEffort::XHigh => {
                gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High)
            }
        }),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn gemini_thinking_to_openai(
    thinking: Option<&gemini::ThinkingConfig>,
) -> Option<openai::ReasoningEffort> {
    let thinking = thinking?;
    if thinking.include_thoughts == Some(false) {
        return Some(openai::ReasoningEffort::None);
    }
    match thinking.thinking_level.as_ref()? {
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal) => {
            Some(openai::ReasoningEffort::Minimal)
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low) => {
            Some(openai::ReasoningEffort::Low)
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Medium)
        | gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::ThinkingLevelUnspecified) => {
            Some(openai::ReasoningEffort::Medium)
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High)
        | gemini::ThinkingLevel::Unknown(_) => Some(openai::ReasoningEffort::High),
    }
}

pub(in crate::transform::generate_content) fn claude_thinking_to_gemini(
    thinking: Option<claude::ThinkingConfig>,
) -> Option<gemini::ThinkingConfig> {
    match thinking? {
        claude::ThinkingConfig::Disabled(_) => Some(gemini::ThinkingConfig {
            include_thoughts: Some(false),
            thinking_budget: None,
            thinking_level: None,
            extra: Default::default(),
        }),
        claude::ThinkingConfig::Enabled(config) => Some(gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: Some(u64_to_i32(config.budget_tokens)),
            thinking_level: None,
            extra: Default::default(),
        }),
        claude::ThinkingConfig::Adaptive(_) => Some(gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: None,
            thinking_level: None,
            extra: Default::default(),
        }),
        claude::ThinkingConfig::Unknown(_) => None,
    }
}

pub(in crate::transform::generate_content) fn gemini_thinking_to_claude(
    thinking: Option<&gemini::ThinkingConfig>,
) -> Option<claude::ThinkingConfig> {
    let thinking = thinking?;
    if thinking.include_thoughts == Some(false) {
        return Some(claude::ThinkingConfig::Disabled(claude::ThinkingDisabled {
            type_: claude::ThinkingDisabledType::Disabled,
            extra: Default::default(),
        }));
    }
    if let Some(budget) = thinking.thinking_budget {
        return Some(claude::ThinkingConfig::Enabled(claude::ThinkingEnabled {
            budget_tokens: i32_to_u64(budget),
            type_: claude::ThinkingEnabledType::Enabled,
            display: None,
            extra: Default::default(),
        }));
    }
    Some(claude::ThinkingConfig::Adaptive(claude::ThinkingAdaptive {
        type_: claude::ThinkingAdaptiveType::Adaptive,
        display: None,
        extra: Default::default(),
    }))
}
