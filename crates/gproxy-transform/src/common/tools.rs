use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native;

pub(crate) fn chat_to_claude(
    tools: Option<Vec<openai::ChatTool>>,
) -> Result<Option<Vec<claude::Tool>>, TransformError> {
    tools
        .map(|tools| tools.into_iter().map(chat_tool_to_claude).collect())
        .transpose()
}

pub(crate) fn claude_to_chat(
    tools: Option<Vec<claude::Tool>>,
) -> Result<Option<Vec<openai::ChatTool>>, TransformError> {
    tools
        .map(|tools| tools.into_iter().map(claude_tool_to_chat).collect())
        .transpose()
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
    tools
        .map(|tools| {
            tools
                .into_iter()
                .map(|tool| match tool {
                    openai::ChatTool::Function(tool) => Ok(openai::ResponseTool::Function {
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
                        rest: merge(tool.rest, tool.function.rest),
                    }),
                    openai::ChatTool::Custom(tool) => Ok(openai::ResponseTool::Custom {
                        name: tool.custom.name,
                        defer_loading: None,
                        description: tool.custom.description,
                        format: tool.custom.format,
                        allowed_callers: None,
                        rest: merge(tool.rest, tool.custom.rest),
                    }),
                    openai::ChatTool::Unknown(raw) => {
                        serde_json::from_value(raw).map_err(TransformError::from)
                    }
                })
                .collect()
        })
        .transpose()
}

pub(crate) fn responses_to_chat(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Result<Option<Vec<openai::ChatTool>>, TransformError> {
    tools
        .map(|tools| {
            tools
                .into_iter()
                .map(|tool| match tool {
                    openai::ResponseTool::Function {
                        name,
                        parameters,
                        strict,
                        description,
                        rest,
                        ..
                    } => Ok(openai::ChatTool::Function(openai::ChatFunctionTool {
                        type_: openai::FunctionToolChoiceType::Function,
                        function: openai::FunctionDefinition {
                            name,
                            description,
                            parameters: match parameters {
                                openai::ResponseFunctionParameters::Schema(schema) => Some(schema),
                                openai::ResponseFunctionParameters::Null => None,
                            },
                            strict: match strict {
                                openai::ResponseFunctionStrict::Value(strict) => Some(strict),
                                openai::ResponseFunctionStrict::Null
                                | openai::ResponseFunctionStrict::Absent => None,
                            },
                            rest,
                        },
                        rest: Default::default(),
                    })),
                    openai::ResponseTool::Custom {
                        name,
                        description,
                        format,
                        rest,
                        ..
                    } => Ok(openai::ChatTool::Custom(openai::ChatCustomTool {
                        type_: openai::CustomToolChoiceType::Custom,
                        custom: openai::CustomToolDefinition {
                            name,
                            description,
                            format,
                            rest,
                        },
                        rest: Default::default(),
                    })),
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
                    | openai::ResponseTool::ApplyPatch { .. }) => Err(TransformError::unsupported(
                        "Responses tool",
                        serde_json::to_string(&unsupported)?,
                    )),
                })
                .collect()
        })
        .transpose()
}

pub(crate) fn chat_choice_to_claude(
    choice: Option<openai::ChatToolChoice>,
    parallel: Option<bool>,
) -> Result<Option<claude::ToolChoice>, TransformError> {
    let disable_parallel_tool_use = parallel.map(|parallel| !parallel);
    Ok(match choice {
        None => None,
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto)) => {
            Some(claude::ToolChoice::Auto(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use,
                rest: Default::default(),
            }))
        }
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Required)) => {
            Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                rest: Default::default(),
            }))
        }
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None)) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                rest: Default::default(),
            }))
        }
        Some(openai::ChatToolChoice::Named(named)) => {
            let (name, rest) = match named {
                openai::ChatNamedToolChoice::Function(choice) => {
                    (choice.function.name, choice.rest)
                }
                openai::ChatNamedToolChoice::Custom(choice) => (choice.custom.name, choice.rest),
                openai::ChatNamedToolChoice::Unknown(raw) => {
                    return Ok(Some(claude::ToolChoice::Unknown(raw)));
                }
            };
            Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                name,
                type_: claude::ToolChoiceToolType::Tool,
                disable_parallel_tool_use,
                rest,
            }))
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
        Some(openai::ChatToolChoice::Unknown(raw)) => Some(claude::ToolChoice::Unknown(raw)),
        Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Unknown(value))) => {
            return Err(TransformError::unsupported(
                "OpenAI Chat tool choice",
                value,
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
            openai::ChatNamedToolChoice::Function(openai::ChatNamedFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::NamedTool {
                    name: choice.name,
                    rest: Default::default(),
                },
                rest: choice.rest,
            }),
        )),
        Some(claude::ToolChoice::Unknown(raw)) => Some(openai::ChatToolChoice::Unknown(raw)),
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
        openai::ChatTool::Function(tool) => Ok(claude::Tool::Custom(claude::CustomTool {
            input_schema: schema_to_claude(tool.function.parameters)?,
            name: tool.function.name,
            type_: Some(claude::CustomToolType::Custom),
            description: tool.function.description,
            eager_input_streaming: None,
            common: claude::ToolCommon {
                strict: tool.function.strict,
                rest: tool.function.rest,
                ..Default::default()
            },
            rest: tool.rest,
        })),
        openai::ChatTool::Custom(tool) => Err(TransformError::unsupported(
            "OpenAI Chat tool",
            format!("custom tool {}", tool.custom.name),
        )),
        openai::ChatTool::Unknown(raw) => Ok(claude::Tool::Unknown(raw)),
    }
}

fn claude_tool_to_chat(tool: claude::Tool) -> Result<openai::ChatTool, TransformError> {
    match tool {
        claude::Tool::Custom(tool) => Ok(openai::ChatTool::Function(openai::ChatFunctionTool {
            type_: openai::FunctionToolChoiceType::Function,
            function: openai::FunctionDefinition {
                name: tool.name,
                description: tool.description,
                parameters: Some(schema_to_openai(tool.input_schema)?),
                strict: tool.common.strict,
                rest: tool.common.rest,
            },
            rest: tool.rest,
        })),
        claude::Tool::Unknown(raw) => Ok(openai::ChatTool::Unknown(raw)),
        other => Err(TransformError::unsupported(
            "Claude tool",
            serde_json::to_string(&other)?,
        )),
    }
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
    callers.map(|callers| {
        callers
            .into_iter()
            .map(|caller| match caller {
                openai::ToolCaller::Direct => claude::ToolCaller::Direct,
                openai::ToolCaller::Programmatic | openai::ToolCaller::Unknown(_) => {
                    claude::ToolCaller::CodeExecution20260120
                }
            })
            .collect()
    })
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

pub(crate) fn merge(
    mut left: serde_json::Map<String, serde_json::Value>,
    right: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    left.extend(right);
    left
}
