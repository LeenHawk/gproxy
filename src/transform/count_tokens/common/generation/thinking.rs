use crate::protocol::{claude, gemini};

use crate::transform::count_tokens::common::scalar::{i32_to_u64, u64_to_i32};

pub(super) fn claude_thinking_to_gemini(
    thinking: claude::ThinkingConfig,
) -> gemini::ThinkingConfig {
    match thinking {
        claude::ThinkingConfig::Enabled(config) => gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: Some(u64_to_i32(config.budget_tokens)),
            thinking_level: None,
            extra: Default::default(),
        },
        claude::ThinkingConfig::Disabled(_) => gemini::ThinkingConfig {
            include_thoughts: Some(false),
            thinking_budget: None,
            thinking_level: None,
            extra: Default::default(),
        },
        claude::ThinkingConfig::Adaptive(_) => gemini::ThinkingConfig {
            include_thoughts: Some(true),
            thinking_budget: None,
            thinking_level: None,
            extra: Default::default(),
        },
        claude::ThinkingConfig::Unknown(_) => gemini::ThinkingConfig::default(),
    }
}

pub(in crate::transform::count_tokens) fn gemini_generation_to_claude_thinking(
    generation_config: Option<&gemini::GenerationConfig>,
) -> Option<claude::ThinkingConfig> {
    let thinking = generation_config?.thinking_config.as_ref()?;
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

pub(super) fn gemini_thinking_to_claude_output_effort(
    thinking: &gemini::ThinkingConfig,
) -> Option<claude::OutputEffort> {
    let effort = match thinking.thinking_level.as_ref()? {
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal)
        | gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low) => {
            claude::OutputEffortKnown::Low
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Medium)
        | gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::ThinkingLevelUnspecified) => {
            claude::OutputEffortKnown::Medium
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High)
        | gemini::ThinkingLevel::Unknown(_) => claude::OutputEffortKnown::High,
    };
    Some(claude::OutputEffort::Known(effort))
}
