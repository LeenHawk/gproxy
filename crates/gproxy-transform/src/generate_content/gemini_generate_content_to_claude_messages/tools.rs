use gproxy_protocol::{claude, gemini};

use crate::TransformError;

mod schema;

pub(super) fn definitions(
    tools: Option<Vec<gemini::Tool>>,
) -> Result<Option<Vec<claude::Tool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        if !tool.rest.is_empty()
            || tool
                .code_execution
                .as_ref()
                .is_some_and(|code| !code.rest.is_empty())
        {
            return Err(TransformError::unsupported("Gemini tool", "tool rest"));
        }
        if let Some(declarations) = tool.function_declarations {
            for declaration in declarations {
                output.push(custom(declaration)?);
            }
        }
        if tool.code_execution.is_some() {
            output.push(bash());
        }
        if tool.google_search_retrieval.is_some()
            || tool.google_search.is_some()
            || tool.computer_use.is_some()
            || tool.url_context.is_some()
            || tool.file_search.is_some()
            || tool.mcp_servers.is_some()
            || tool.google_maps.is_some()
        {
            return Err(TransformError::unsupported(
                "Gemini tool",
                "a tool without a Claude counterpart",
            ));
        }
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(super) fn choice(
    config: Option<gemini::ToolConfig>,
) -> Result<Option<claude::ToolChoice>, TransformError> {
    let Some(config) = config.and_then(|config| config.function_calling_config) else {
        return Ok(None);
    };
    let Some(mode) = config.mode else {
        return Ok(None);
    };
    Ok(match mode {
        gemini::FunctionCallingMode::Known(
            gemini::FunctionCallingModeKnown::Auto
            | gemini::FunctionCallingModeKnown::ModeUnspecified,
        ) => Some(claude::ToolChoice::Auto(claude::ToolChoiceAuto {
            type_: claude::ToolChoiceAutoType::Auto,
            disable_parallel_tool_use: None,
            rest: config.rest,
        })),
        gemini::FunctionCallingMode::Known(
            gemini::FunctionCallingModeKnown::Any | gemini::FunctionCallingModeKnown::Validated,
        ) => match config.allowed_function_names {
            Some(names) if names.len() > 1 => {
                return Err(TransformError::unsupported(
                    "Gemini function calling config",
                    "multiple allowed function names",
                ));
            }
            Some(names) => match names.into_iter().next() {
                Some(name) => Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                    name,
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use: None,
                    rest: config.rest,
                })),
                None => Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use: None,
                    rest: config.rest,
                })),
            },
            None => Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use: None,
                rest: config.rest,
            })),
        },
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                rest: config.rest,
            }))
        }
        gemini::FunctionCallingMode::Unknown(value) => {
            return Err(TransformError::unsupported(
                "Gemini function calling mode",
                value,
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Gemini tool choice",
                "future mode",
            ));
        }
    })
}

fn custom(declaration: gemini::FunctionDeclaration) -> Result<claude::Tool, TransformError> {
    if declaration.behavior.is_some()
        || declaration.response.is_some()
        || declaration.response_json_schema.is_some()
        || !declaration.rest.is_empty()
    {
        return Err(TransformError::unsupported(
            "Gemini function declaration",
            "fields without a Claude counterpart",
        ));
    }
    let schema = match declaration.parameters_json_schema {
        Some(value) => schema::convert(value)?,
        None => match declaration.parameters {
            Some(value) => schema::convert(serde_json::to_value(value)?)?,
            None => schema::empty(),
        },
    };
    Ok(claude::Tool::Custom(claude::CustomTool {
        input_schema: schema,
        name: declaration.name,
        type_: Some(claude::CustomToolType::Custom),
        description: Some(declaration.description),
        eager_input_streaming: None,
        common: claude::ToolCommon::default(),
        rest: Default::default(),
    }))
}

fn bash() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: claude::ToolCommon::default(),
            rest: Default::default(),
        },
    ))
}
