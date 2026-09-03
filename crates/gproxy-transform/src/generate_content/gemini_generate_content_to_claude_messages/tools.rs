use gproxy_protocol::{claude, gemini};

use crate::TransformError;

mod schema;

pub(super) fn typed_schema_value(
    schema: gemini::Schema,
) -> Result<serde_json::Value, TransformError> {
    schema::typed_value(schema)
}

pub(super) fn definitions(
    tools: Option<Vec<gemini::Tool>>,
) -> Result<Option<Vec<claude::Tool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        if let Some(declarations) = tool.function_declarations {
            for declaration in declarations {
                output.push(custom(declaration)?);
            }
        }
        if tool.code_execution.is_some() {
            output.push(bash());
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
        ) => Some(claude::ToolChoice::Auto(crate::wire!(
            claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use: None,
                rest: Default::default(),
            }
        ))),
        gemini::FunctionCallingMode::Known(
            gemini::FunctionCallingModeKnown::Any | gemini::FunctionCallingModeKnown::Validated,
        ) => match config.allowed_function_names {
            Some(names) if names.len() > 1 => Some(claude::ToolChoice::Any(crate::wire!(
                claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use: None,
                    rest: Default::default(),
                }
            ))),
            Some(names) => match names.into_iter().next() {
                Some(name) => Some(claude::ToolChoice::Tool(crate::wire!(
                    claude::ToolChoiceTool {
                        name,
                        type_: claude::ToolChoiceToolType::Tool,
                        disable_parallel_tool_use: None,
                        rest: Default::default(),
                    }
                ))),
                None => Some(claude::ToolChoice::Any(crate::wire!(
                    claude::ToolChoiceAny {
                        type_: claude::ToolChoiceAnyType::Any,
                        disable_parallel_tool_use: None,
                        rest: Default::default(),
                    }
                ))),
            },
            None => Some(claude::ToolChoice::Any(crate::wire!(
                claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use: None,
                    rest: Default::default(),
                }
            ))),
        },
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => Some(
            claude::ToolChoice::None(crate::wire!(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                rest: Default::default(),
            })),
        ),
        gemini::FunctionCallingMode::Unknown(_) => None,
        _ => None,
    })
}

fn custom(declaration: gemini::FunctionDeclaration) -> Result<claude::Tool, TransformError> {
    let schema = match declaration.parameters_json_schema {
        Some(value) => schema::convert(value)?,
        None => match declaration.parameters {
            Some(value) => schema::convert(schema::typed_value(value)?)?,
            None => schema::empty(),
        },
    };
    Ok(claude::Tool::Custom(crate::wire!(claude::CustomTool {
        input_schema: schema,
        name: declaration.name,
        type_: Some(claude::CustomToolType::Custom),
        description: Some(declaration.description),
        eager_input_streaming: None,
        common: claude::ToolCommon::default(),
        rest: Default::default(),
    })))
}

fn bash() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(crate::wire!(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: claude::ToolCommon::default(),
            rest: Default::default(),
        }
    )))
}
