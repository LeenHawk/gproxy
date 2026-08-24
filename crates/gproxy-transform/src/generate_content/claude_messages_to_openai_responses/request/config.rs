use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(super) fn system_text(system: claude::SystemPrompt) -> Result<String, TransformError> {
    match system {
        claude::StringOrArray::String(text) => Ok(text),
        claude::StringOrArray::Array(blocks) => Ok(blocks
            .into_iter()
            .map(|block| block.text)
            .collect::<String>()),
        claude::StringOrArray::Raw(raw) => Err(TransformError::unsupported(
            "Claude system",
            raw.to_string(),
        )),
        _ => Err(TransformError::unsupported(
            "Claude system",
            "future system shape",
        )),
    }
}

pub(super) fn tool_choice(
    choice: Option<claude::ToolChoice>,
) -> Result<Option<openai::ResponseToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(claude::ToolChoice::Auto(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Auto,
        )),
        Some(claude::ToolChoice::Any(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        Some(claude::ToolChoice::None(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::None,
        )),
        Some(claude::ToolChoice::Tool(choice)) => Some(openai::ResponseToolChoice::Function(
            openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.name,
                rest: choice.rest,
            },
        )),
        Some(claude::ToolChoice::Unknown(raw)) => Some(openai::ResponseToolChoice::Unknown(raw)),
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool choice",
                "future choice",
            ));
        }
    })
}

pub(super) fn parallel(choice: &Option<claude::ToolChoice>) -> Option<bool> {
    match choice {
        Some(claude::ToolChoice::Auto(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Any(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Tool(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        _ => None,
    }
}

pub(super) fn reasoning(
    output: Option<&claude::OutputConfig>,
    thinking: Option<&claude::ThinkingConfig>,
) -> Result<Option<openai::ReasoningConfig>, TransformError> {
    let effort = output
        .and_then(|output| output.effort.as_ref())
        .map(|effort| serde_json::from_value(serde_json::to_value(effort)?))
        .transpose()?;
    let effort = effort.or(match thinking {
        Some(claude::ThinkingConfig::Disabled(_)) => Some(openai::ReasoningEffort::None),
        Some(claude::ThinkingConfig::Enabled(_) | claude::ThinkingConfig::Adaptive(_)) => {
            Some(openai::ReasoningEffort::Medium)
        }
        Some(claude::ThinkingConfig::Unknown(_)) | Some(_) | None => None,
    });
    Ok(effort.map(|effort| openai::ReasoningConfig {
        context: None,
        effort: Some(effort),
        mode: None,
        summary: None,
        generate_summary: None,
        rest: Default::default(),
    }))
}

pub(super) fn text_config(
    output: Option<&claude::OutputConfig>,
    legacy: Option<&claude::JsonSchemaFormat>,
) -> Result<Option<openai::TextConfig>, TransformError> {
    let format = output.and_then(|output| output.format.as_ref()).or(legacy);
    Ok(format.map(|format| openai::TextConfig {
        format: Some(openai::ResponseFormat::JsonSchema(
            openai::JsonSchemaResponseFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                name: "response".into(),
                schema: format.schema.clone(),
                description: None,
                strict: None,
                rest: format.rest.clone(),
            },
        )),
        verbosity: None,
        rest: Default::default(),
    }))
}

pub(super) fn service_tier(
    tier: Option<claude::RequestServiceTier>,
    speed: Option<claude::Speed>,
) -> Result<Option<openai::ServiceTier>, TransformError> {
    if matches!(speed, Some(claude::Speed::Known(claude::SpeedKnown::Fast))) {
        return Ok(Some(openai::ServiceTier::Fast));
    }
    Ok(tier
        .map(|tier| serde_json::from_value(serde_json::to_value(tier)?))
        .transpose()?)
}
