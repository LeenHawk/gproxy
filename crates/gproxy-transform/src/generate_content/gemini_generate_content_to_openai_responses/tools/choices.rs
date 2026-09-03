use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn choice_to_responses(
    config: Option<gemini::ToolConfig>,
) -> Result<Option<openai::ResponseToolChoice>, TransformError> {
    let Some(config) = config else {
        return Ok(None);
    };
    if config.retrieval_config.is_some() || config.include_server_side_tool_invocations.is_some() {
        return Err(TransformError::unsupported(
            "Gemini toolConfig",
            "retrieval, server-side invocation, or extension settings",
        ));
    }
    let Some(config) = config.function_calling_config else {
        return Ok(None);
    };
    let names = config.allowed_function_names;
    Ok(match config.mode {
        None
        | Some(gemini::FunctionCallingMode::Known(
            gemini::FunctionCallingModeKnown::ModeUnspecified,
        )) => {
            if names.is_some() {
                return Err(TransformError::unsupported(
                    "Gemini functionCallingConfig",
                    "allowed names without an active mode",
                ));
            }
            None
        }
        Some(gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None)) => {
            if names.is_some() {
                return Err(TransformError::unsupported(
                    "Gemini functionCallingConfig",
                    "allowed names with NONE mode",
                ));
            }
            Some(openai::ResponseToolChoice::Mode(
                openai::ToolChoiceMode::None,
            ))
        }
        Some(gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)) => {
            Some(choice_with_names(openai::AllowedToolsMode::Auto, names))
        }
        Some(gemini::FunctionCallingMode::Known(
            gemini::FunctionCallingModeKnown::Any | gemini::FunctionCallingModeKnown::Validated,
        )) => Some(choice_with_names(openai::AllowedToolsMode::Required, names)),
        Some(gemini::FunctionCallingMode::Unknown(_)) => {
            if names.is_some() {
                return Err(TransformError::unsupported(
                    "Gemini functionCallingConfig",
                    "allowed names with an unknown mode",
                ));
            }
            None
        }
        Some(_) => {
            return Err(TransformError::unsupported(
                "Gemini function calling mode",
                "future mode",
            ));
        }
    })
}

fn choice_with_names(
    mode: openai::AllowedToolsMode,
    names: Option<Vec<String>>,
) -> openai::ResponseToolChoice {
    let Some(names) = names else {
        return openai::ResponseToolChoice::Mode(match mode {
            openai::AllowedToolsMode::Auto => openai::ToolChoiceMode::Auto,
            openai::AllowedToolsMode::Required | openai::AllowedToolsMode::Unknown(_) => {
                openai::ToolChoiceMode::Required
            }
        });
    };
    openai::ResponseToolChoice::Allowed(openai::ResponseAllowedToolChoice {
        mode,
        tools: names
            .into_iter()
            .map(|name| openai::ResponseAllowedTool::Function {
                name,
                rest: Default::default(),
            })
            .collect(),
        type_: openai::AllowedToolsType::AllowedTools,
        rest: Default::default(),
    })
}
