//! Portable reasoning controls; explicit native options retain precedence.
use crate::*;
fn warn(field: &str, message: &str) -> Warning {
    Warning::new(WarningKind::Compatibility, field, message)
}
fn unsupported(field: &str, message: &str) -> Warning {
    Warning::new(WarningKind::Unsupported, field, message)
}
#[cfg(feature = "claude")]
pub(crate) fn claude(
    request: &GenerateRequest,
    body: &mut protocol::claude::generate_content::GenerateContentRequestBody,
) -> Vec<Warning> {
    use protocol::claude::count_tokens as c;
    let Some(setting) = &request.settings.reasoning else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    let options = &request.provider_options.claude;
    if options.thinking.is_some()
        || options
            .output_config
            .as_ref()
            .and_then(|config| config.effort)
            .is_some()
    {
        if setting.effort != ReasoningEffort::ProviderDefault || setting.token_budget.is_some() {
            warnings.push(warn(
                "settings.reasoning",
                "Claude native reasoning options override portable reasoning",
            ));
        }
        return warnings;
    }
    if setting.effort == ReasoningEffort::None {
        body.thinking = Some(c::ThinkingConfig::Disabled(
            c::ThinkingDisabled::builder().build(),
        ));
        if setting.token_budget.is_some() {
            warnings.push(unsupported(
                "settings.reasoning.token_budget",
                "A budget is ignored when reasoning is disabled",
            ));
        }
        return warnings;
    }
    if let Some(budget) = setting.token_budget {
        body.thinking = Some(c::ThinkingConfig::Enabled(
            c::ThinkingEnabled::builder(i64::from(budget)).build(),
        ));
        if setting.effort != ReasoningEffort::ProviderDefault {
            warnings.push(warn("settings.reasoning.effort","The explicit token budget selects Claude budgeted thinking instead of adaptive effort"));
        }
        return warnings;
    }
    let effort = match &setting.effort {
        ReasoningEffort::ProviderDefault => return warnings,
        ReasoningEffort::Minimal => {
            warnings.push(warn(
                "settings.reasoning.effort",
                "Claude maps minimal effort to low",
            ));
            c::Effort::Low
        }
        ReasoningEffort::Low => c::Effort::Low,
        ReasoningEffort::Medium => c::Effort::Medium,
        ReasoningEffort::High => c::Effort::High,
        ReasoningEffort::Xhigh => c::Effort::Xhigh,
        ReasoningEffort::Max => c::Effort::Max,
        ReasoningEffort::Unknown(_) => {
            warnings.push(unsupported(
                "settings.reasoning.effort",
                "Unknown reasoning effort is not sent",
            ));
            return warnings;
        }
        ReasoningEffort::None => return warnings,
    };
    body.thinking = Some(c::ThinkingConfig::Adaptive(
        c::ThinkingAdaptive::builder().build(),
    ));
    let config = body
        .output_config
        .get_or_insert_with(|| c::OutputConfig::builder().build());
    config.effort = Some(effort);
    warnings
}
#[cfg(feature = "openai")]
pub(crate) fn openai(
    request: &GenerateRequest,
    body: &mut protocol::openai::responses::generate::GenerateContentRequestBody,
) -> Vec<Warning> {
    use protocol::openai::responses::input as o;
    let Some(setting) = &request.settings.reasoning else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if setting.token_budget.is_some() {
        warnings.push(unsupported(
            "settings.reasoning.token_budget",
            "OpenAI Responses has no reasoning token-budget field",
        ));
    }
    let effort = match &setting.effort {
        ReasoningEffort::ProviderDefault => return warnings,
        ReasoningEffort::None => o::ReasoningEffort::None,
        ReasoningEffort::Minimal => o::ReasoningEffort::Minimal,
        ReasoningEffort::Low => o::ReasoningEffort::Low,
        ReasoningEffort::Medium => o::ReasoningEffort::Medium,
        ReasoningEffort::High => o::ReasoningEffort::High,
        ReasoningEffort::Xhigh => o::ReasoningEffort::Xhigh,
        ReasoningEffort::Max => o::ReasoningEffort::Max,
        ReasoningEffort::Unknown(_) => {
            warnings.push(unsupported(
                "settings.reasoning.effort",
                "Unknown reasoning effort is not sent",
            ));
            return warnings;
        }
    };
    let config = body
        .reasoning
        .get_or_insert(None)
        .get_or_insert_with(|| o::ReasoningConfig::builder().build());
    if config.effort.as_ref().and_then(Option::as_ref).is_some() {
        warnings.push(warn(
            "settings.reasoning.effort",
            "OpenAI reasoning.effort overrides portable effort",
        ));
    } else {
        config.effort = Some(Some(effort));
    }
    warnings
}
#[cfg(feature = "chat")]
pub(crate) fn chat(
    request: &GenerateRequest,
    body: &mut protocol::openai::chat::request::GenerateContentRequestBody,
) -> Vec<Warning> {
    use protocol::openai::chat::request::ReasoningEffort as E;
    let Some(setting) = &request.settings.reasoning else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if setting.token_budget.is_some() {
        warnings.push(unsupported(
            "settings.reasoning.token_budget",
            "OpenAI Chat has no portable reasoning token-budget field",
        ));
    }
    let effort = match &setting.effort {
        ReasoningEffort::ProviderDefault => return warnings,
        ReasoningEffort::None => E::None,
        ReasoningEffort::Minimal => E::Minimal,
        ReasoningEffort::Low => E::Low,
        ReasoningEffort::Medium => E::Medium,
        ReasoningEffort::High => E::High,
        ReasoningEffort::Xhigh => E::XHigh,
        ReasoningEffort::Max => E::Max,
        ReasoningEffort::Unknown(_) => {
            warnings.push(unsupported(
                "settings.reasoning.effort",
                "Unknown reasoning effort is not sent",
            ));
            return warnings;
        }
    };
    if body.reasoning_effort.flatten().is_some() {
        warnings.push(warn(
            "settings.reasoning.effort",
            "OpenAI Chat reasoning_effort overrides portable effort",
        ));
    } else {
        body.reasoning_effort = Some(Some(effort));
    }
    warnings
}
#[cfg(feature = "gemini")]
pub(crate) fn gemini(
    request: &GenerateRequest,
    body: &mut protocol::gemini::GenerateContentRequestBody,
) -> Vec<Warning> {
    use protocol::gemini::{GenerationConfig, ThinkingConfig, ThinkingLevel as L};
    let Some(setting) = &request.settings.reasoning else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    if setting.effort == ReasoningEffort::ProviderDefault && setting.token_budget.is_none() {
        return warnings;
    }
    let config = body
        .generation_config
        .get_or_insert_with(|| GenerationConfig::builder().build())
        .thinking_config
        .get_or_insert_with(|| ThinkingConfig::builder().build());
    if config.thinking_budget.is_some() || config.thinking_level.is_some() {
        warnings.push(warn(
            "settings.reasoning",
            "Gemini thinkingConfig overrides portable reasoning",
        ));
        return warnings;
    }
    if setting.effort == ReasoningEffort::None {
        config.thinking_budget = Some(0);
        if setting.token_budget.is_some() {
            warnings.push(unsupported(
                "settings.reasoning.token_budget",
                "A budget is ignored when reasoning is disabled",
            ));
        }
    } else if let Some(budget) = setting.token_budget {
        config.thinking_budget = Some(i64::from(budget));
        if setting.effort != ReasoningEffort::ProviderDefault {
            warnings.push(warn(
                "settings.reasoning.effort",
                "The explicit token budget takes precedence over Gemini thinking level",
            ));
        }
    } else {
        config.thinking_level = match &setting.effort {
            ReasoningEffort::Minimal => Some(L::Minimal),
            ReasoningEffort::Low => Some(L::Low),
            ReasoningEffort::Medium => Some(L::Medium),
            ReasoningEffort::High => Some(L::High),
            ReasoningEffort::Xhigh | ReasoningEffort::Max => {
                warnings.push(warn(
                    "settings.reasoning.effort",
                    "Gemini maps xhigh/max effort to high",
                ));
                Some(L::High)
            }
            ReasoningEffort::Unknown(_) => {
                warnings.push(unsupported(
                    "settings.reasoning.effort",
                    "Unknown reasoning effort is not sent",
                ));
                None
            }
            ReasoningEffort::ProviderDefault | ReasoningEffort::None => None,
        };
    }
    warnings
}
