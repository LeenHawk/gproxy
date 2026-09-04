use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native;

pub(crate) fn chat_to_claude(
    tools: Option<Vec<openai::ChatTool>>,
) -> Result<Option<Vec<claude::Tool>>, TransformError> {
    filter_tools(tools, chat_tool_to_claude)
}

pub(crate) fn claude_to_chat(
    tools: Option<Vec<claude::Tool>>,
) -> Result<Option<Vec<openai::ChatTool>>, TransformError> {
    filter_tools(tools, claude_tool_to_chat)
}

pub(crate) fn responses_to_claude(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Result<Option<Vec<claude::Tool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        match native::definitions::response_to_claude(tool) {
            Ok(tool) => output.push(tool),
            Err(TransformError::Unsupported { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn claude_to_responses(
    tools: Option<Vec<claude::Tool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        match native::definitions::claude_to_response(tool) {
            Ok(tool) => output.push(tool),
            Err(TransformError::Unsupported { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn chat_to_responses(
    tools: Option<Vec<openai::ChatTool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let output = tools
        .into_iter()
        .filter_map(|tool| match tool {
            openai::ChatTool::Function(tool) => Some(openai::ResponseTool::Function {
                name: tool.function.name,
                parameters: tool
                    .function
                    .parameters
                    .map(openai::ResponseFunctionParameters::Schema)
                    .unwrap_or(openai::ResponseFunctionParameters::Null),
                strict: tool
                    .function
                    .strict
                    .map(openai::ResponseFunctionStrict::Value)
                    .unwrap_or(openai::ResponseFunctionStrict::Absent),
                defer_loading: None,
                description: tool.function.description,
                output_schema: None,
                allowed_callers: None,
                async_: None,
                rest: Default::default(),
            }),
            openai::ChatTool::Custom(tool) => Some(openai::ResponseTool::Custom {
                name: tool.custom.name,
                defer_loading: None,
                description: tool.custom.description,
                format: tool.custom.format,
                allowed_callers: None,
                async_: None,
                rest: Default::default(),
            }),
            openai::ChatTool::Unknown(_) => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn responses_to_chat(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Result<Option<Vec<openai::ChatTool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let output = tools
        .into_iter()
        .filter_map(|tool| match tool {
            openai::ResponseTool::Function {
                name,
                parameters,
                strict,
                description,
                ..
            } => Some(openai::ChatTool::Function(crate::wire!(
                openai::ChatFunctionTool {
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::FunctionDefinition {
                        name,
                        description,
                        parameters: match parameters {
                            openai::ResponseFunctionParameters::Schema(schema) => Some(schema),
                            openai::ResponseFunctionParameters::Null => None,
                            #[cfg(not(feature = "exhaustive"))]
                            _ => None,
                        },
                        strict: match strict {
                            openai::ResponseFunctionStrict::Value(strict) => Some(strict),
                            openai::ResponseFunctionStrict::Null
                            | openai::ResponseFunctionStrict::Absent => None,
                            #[cfg(not(feature = "exhaustive"))]
                            _ => None,
                        },
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }
            ))),
            openai::ResponseTool::Custom {
                name,
                description,
                format,
                ..
            } => Some(openai::ChatTool::Custom(crate::wire!(
                openai::ChatCustomTool {
                    type_: openai::CustomToolChoiceType::Custom,
                    custom: openai::CustomToolDefinition {
                        name,
                        description,
                        format,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }
            ))),
            unsupported @ (openai::ResponseTool::FileSearch { .. }
            | openai::ResponseTool::Computer { .. }
            | openai::ResponseTool::ComputerUsePreview { .. }
            | openai::ResponseTool::WebSearch { .. }
            | openai::ResponseTool::WebSearch20250826 { .. }
            | openai::ResponseTool::WebFetch { .. }
            | openai::ResponseTool::Memory { .. }
            | openai::ResponseTool::XSearch { .. }
            | openai::ResponseTool::CollectionsSearch { .. }
            | openai::ResponseTool::Mcp { .. }
            | openai::ResponseTool::CodeExecution { .. }
            | openai::ResponseTool::CodeInterpreter { .. }
            | openai::ResponseTool::ImageGeneration { .. }
            | openai::ResponseTool::LocalShell { .. }
            | openai::ResponseTool::Shell { .. }
            | openai::ResponseTool::Namespace { .. }
            | openai::ResponseTool::ToolSearch { .. }
            | openai::ResponseTool::ProgrammaticToolCalling { .. }
            | openai::ResponseTool::WebSearchPreview { .. }
            | openai::ResponseTool::WebSearchPreview20250311 { .. }
            | openai::ResponseTool::ApplyPatch { .. }) => {
                let _ = unsupported;
                None
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => None,
        })
        .collect::<Vec<_>>();
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn chat_choice_to_claude(
    choice: Option<openai::ChatToolChoice>,
    parallel: Option<bool>,
) -> Result<Option<claude::ToolChoice>, TransformError> {
    let disable_parallel_tool_use = parallel.map(|parallel| !parallel);
    Ok(match choice {
        None => None,
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto)) => Some(
            claude::ToolChoice::Auto(crate::wire!(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use,
                rest: Default::default(),
            })),
        ),
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Required)) => Some(
            claude::ToolChoice::Any(crate::wire!(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                rest: Default::default(),
            })),
        ),
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None)) => Some(
            claude::ToolChoice::None(crate::wire!(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                rest: Default::default(),
            })),
        ),
        Some(openai::ChatToolChoice::Named(named)) => {
            let name = match named {
                openai::ChatNamedToolChoice::Function(choice) => choice.function.name,
                openai::ChatNamedToolChoice::Custom(choice) => choice.custom.name,
                openai::ChatNamedToolChoice::Unknown(_) => return Ok(None),
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            };
            Some(claude::ToolChoice::Tool(crate::wire!(
                claude::ToolChoiceTool {
                    name,
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use,
                    rest: Default::default(),
                }
            )))
        }
        Some(openai::ChatToolChoice::Allowed(choice)) => {
            return Err(TransformError::unsupported(
                "OpenAI Chat tool choice",
                format!(
                    "allowed tools: {} entries",
                    choice.allowed_tools.tools.len()
                ),
            ));
        }
        Some(openai::ChatToolChoice::Unknown(_)) => None,
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Unknown(value))) => {
            return Err(TransformError::unsupported(
                "OpenAI Chat tool choice",
                value,
            ));
        }
        #[cfg(not(feature = "exhaustive"))]
        Some(_) => {
            return Err(TransformError::unsupported(
                "OpenAI Chat tool choice",
                "unrecognized external variant",
            ));
        }
    })
}

pub(crate) fn claude_choice_to_chat(
    choice: Option<claude::ToolChoice>,
) -> Result<Option<openai::ChatToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(claude::ToolChoice::Auto(_choice)) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto))
        }
        Some(claude::ToolChoice::Any(_choice)) => Some(openai::ChatToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        Some(claude::ToolChoice::None(_)) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None))
        }
        Some(claude::ToolChoice::Tool(choice)) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Function(crate::wire!(
                openai::ChatNamedFunctionToolChoice {
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::NamedTool {
                        name: choice.name,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }
            )),
        )),
        Some(claude::ToolChoice::Unknown(_)) => None,
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool choice",
                "future tool choice",
            ));
        }
    })
}

fn chat_tool_to_claude(tool: openai::ChatTool) -> Result<claude::Tool, TransformError> {
    match tool {
        openai::ChatTool::Function(tool) => {
            Ok(claude::Tool::Custom(crate::wire!(claude::CustomTool {
                input_schema: schema_to_claude(tool.function.parameters)?,
                name: tool.function.name,
                type_: Some(claude::CustomToolType::Custom),
                description: tool.function.description,
                eager_input_streaming: None,
                common: claude::ToolCommon {
                    strict: tool.function.strict,
                    rest: Default::default(),
                    ..Default::default()
                },
                rest: Default::default(),
            })))
        }
        openai::ChatTool::Custom(tool) => Err(TransformError::unsupported(
            "OpenAI Chat tool",
            format!("custom tool {}", tool.custom.name),
        )),
        openai::ChatTool::Unknown(_) => Err(TransformError::unsupported(
            "OpenAI Chat tool",
            "unknown tool",
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn claude_tool_to_chat(tool: claude::Tool) -> Result<openai::ChatTool, TransformError> {
    match tool {
        claude::Tool::Custom(tool) => Ok(openai::ChatTool::Function(crate::wire!(
            openai::ChatFunctionTool {
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionDefinition {
                    name: tool.name,
                    description: tool.description,
                    parameters: Some(schema_to_openai(tool.input_schema)?),
                    strict: tool.common.strict,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }
        ))),
        claude::Tool::Unknown(_) => Err(TransformError::unsupported("Claude tool", "unknown tool")),
        other => Err(TransformError::unsupported(
            "Claude tool",
            serde_json::to_string(&other)?,
        )),
    }
}

fn filter_tools<S, T>(
    tools: Option<Vec<S>>,
    convert: impl Fn(S) -> Result<T, TransformError>,
) -> Result<Option<Vec<T>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        match convert(tool) {
            Ok(tool) => output.push(tool),
            Err(TransformError::Unsupported { .. }) => {}
            Err(error) => return Err(error),
        }
    }
    Ok((!output.is_empty()).then_some(output))
}

fn schema_to_claude(
    schema: Option<openai::JsonSchema>,
) -> Result<claude::JsonSchema, TransformError> {
    let schema =
        schema.ok_or_else(|| TransformError::shape("OpenAI tool", "input schema is missing"))?;
    Ok(serde_json::from_value(serde_json::Value::Object(schema))?)
}

fn schema_to_openai(schema: claude::JsonSchema) -> Result<openai::JsonSchema, TransformError> {
    serde_json::to_value(schema)?
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("JSON schema", "expected an object"))
}

pub(crate) fn callers_to_claude(
    callers: Option<Vec<openai::ToolCaller>>,
) -> Option<Vec<claude::ToolCaller>> {
    callers
        .filter(|callers| !callers.is_empty())
        .map(|_| vec![claude::ToolCaller::Direct])
}

pub(crate) fn callers_to_openai(
    callers: Option<Vec<claude::ToolCaller>>,
) -> Option<Vec<openai::ToolCaller>> {
    callers.map(|callers| {
        callers
            .into_iter()
            .map(|caller| match caller {
                claude::ToolCaller::Direct => openai::ToolCaller::Direct,
                _ => openai::ToolCaller::Programmatic,
            })
            .collect()
    })
}
