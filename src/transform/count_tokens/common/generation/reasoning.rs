use crate::protocol::{claude, gemini, openai};

pub(super) fn openai_reasoning_effort_to_gemini(
    effort: openai::ReasoningEffort,
) -> gemini::ThinkingLevel {
    let level = match effort {
        openai::ReasoningEffort::None | openai::ReasoningEffort::Minimal => {
            gemini::ThinkingLevelKnown::Minimal
        }
        openai::ReasoningEffort::Low => gemini::ThinkingLevelKnown::Low,
        openai::ReasoningEffort::Medium => gemini::ThinkingLevelKnown::Medium,
        openai::ReasoningEffort::High | openai::ReasoningEffort::XHigh => {
            gemini::ThinkingLevelKnown::High
        }
    };
    gemini::ThinkingLevel::Known(level)
}

pub(in crate::transform::count_tokens) fn claude_generation_to_openai_reasoning(
    thinking: Option<claude::ThinkingConfig>,
    output_config: Option<&claude::OutputConfig>,
) -> Option<openai::ReasoningConfig> {
    let effort = output_config
        .and_then(|config| config.effort.clone())
        .map(claude_output_effort_to_openai)
        .or_else(|| thinking.map(claude_thinking_to_openai_effort));

    effort.map(|effort| openai::ReasoningConfig {
        effort: Some(effort),
        summary: None,
        generate_summary: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn gemini_generation_to_openai_reasoning(
    generation_config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ReasoningConfig> {
    let thinking = generation_config?.thinking_config.as_ref()?;
    let effort = if thinking.include_thoughts == Some(false) {
        openai::ReasoningEffort::None
    } else {
        thinking
            .thinking_level
            .clone()
            .map(gemini_thinking_level_to_openai)
            .unwrap_or(openai::ReasoningEffort::Medium)
    };
    Some(openai::ReasoningConfig {
        effort: Some(effort),
        summary: None,
        generate_summary: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn openai_reasoning_to_claude(
    reasoning: Option<openai::ReasoningConfig>,
) -> Option<claude::ThinkingConfig> {
    match reasoning?.effort? {
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

pub(super) fn openai_reasoning_effort_to_claude_output(
    effort: &openai::ReasoningEffort,
) -> claude::OutputEffort {
    let effort = match effort {
        openai::ReasoningEffort::None
        | openai::ReasoningEffort::Minimal
        | openai::ReasoningEffort::Low => claude::OutputEffortKnown::Low,
        openai::ReasoningEffort::Medium => claude::OutputEffortKnown::Medium,
        openai::ReasoningEffort::High => claude::OutputEffortKnown::High,
        openai::ReasoningEffort::XHigh => claude::OutputEffortKnown::XHigh,
    };
    claude::OutputEffort::Known(effort)
}

fn claude_output_effort_to_openai(effort: claude::OutputEffort) -> openai::ReasoningEffort {
    match effort {
        claude::OutputEffort::Known(claude::OutputEffortKnown::Low) => openai::ReasoningEffort::Low,
        claude::OutputEffort::Known(claude::OutputEffortKnown::Medium) => {
            openai::ReasoningEffort::Medium
        }
        claude::OutputEffort::Known(claude::OutputEffortKnown::High) => {
            openai::ReasoningEffort::High
        }
        claude::OutputEffort::Known(claude::OutputEffortKnown::XHigh)
        | claude::OutputEffort::Known(claude::OutputEffortKnown::Max)
        | claude::OutputEffort::Unknown(_) => openai::ReasoningEffort::XHigh,
    }
}

fn claude_thinking_to_openai_effort(thinking: claude::ThinkingConfig) -> openai::ReasoningEffort {
    match thinking {
        claude::ThinkingConfig::Disabled(_) => openai::ReasoningEffort::None,
        claude::ThinkingConfig::Enabled(_) => openai::ReasoningEffort::Medium,
        claude::ThinkingConfig::Adaptive(_) => openai::ReasoningEffort::Medium,
        claude::ThinkingConfig::Unknown(_) => openai::ReasoningEffort::Medium,
    }
}

fn gemini_thinking_level_to_openai(level: gemini::ThinkingLevel) -> openai::ReasoningEffort {
    match level {
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Minimal) => {
            openai::ReasoningEffort::Minimal
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Low) => {
            openai::ReasoningEffort::Low
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::Medium)
        | gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::ThinkingLevelUnspecified) => {
            openai::ReasoningEffort::Medium
        }
        gemini::ThinkingLevel::Known(gemini::ThinkingLevelKnown::High)
        | gemini::ThinkingLevel::Unknown(_) => openai::ReasoningEffort::High,
    }
}
