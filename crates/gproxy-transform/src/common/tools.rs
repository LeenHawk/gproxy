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
    tools
        .map(|tools| tools.into_iter().map(response_tool_to_claude).collect())
        .transpose()
}

pub(crate) fn claude_to_responses(
    tools: Option<Vec<claude::Tool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    tools
        .map(|tools| tools.into_iter().map(claude_tool_to_response).collect())
        .transpose()
}

pub(crate) fn chat_to_responses(
    tools: Option<Vec<openai::ChatTool>>,
) -> Result<Option<Vec<openai::ResponseTool>>, TransformError> {
    tools
        .map(|tools| {
            tools
                .into_iter()
                .map(|tool| match tool {
                    openai::ChatTool::Function(tool) => Ok(openai::ResponseTool {
                        type_: openai::ToolType::Function,
                        name: Some(tool.function.name),
                        parameters: tool.function.parameters,
                        strict: tool.function.strict,
                        description: tool.function.description,
                        rest: merge(tool.rest, tool.function.rest),
                        ..empty_response_tool()
                    }),
                    openai::ChatTool::Custom(tool) => Ok(openai::ResponseTool {
                        type_: openai::ToolType::Custom,
                        name: Some(tool.custom.name),
                        description: tool.custom.description,
                        format: tool.custom.format,
                        rest: merge(tool.rest, tool.custom.rest),
                        ..empty_response_tool()
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
                .map(|tool| match tool.type_ {
                    openai::ToolType::Function => {
                        Ok(openai::ChatTool::Function(openai::ChatFunctionTool {
                            type_: openai::FunctionToolChoiceType::Function,
                            function: openai::FunctionDefinition {
                                name: tool.name.ok_or_else(|| {
                                    TransformError::shape("Responses tool", "name is missing")
                                })?,
                                description: tool.description,
                                parameters: tool.parameters,
                                strict: tool.strict,
                                rest: tool.rest,
                            },
                            rest: Default::default(),
                        }))
                    }
                    openai::ToolType::Custom => {
                        Ok(openai::ChatTool::Custom(openai::ChatCustomTool {
                            type_: openai::CustomToolChoiceType::Custom,
                            custom: openai::CustomToolDefinition {
                                name: tool.name.ok_or_else(|| {
                                    TransformError::shape("Responses tool", "name is missing")
                                })?,
                                description: tool.description,
                                format: tool.format,
                                rest: tool.rest,
                            },
                            rest: Default::default(),
                        }))
                    }
                    _ => Err(TransformError::unsupported(
                        "Responses tool",
                        serde_json::to_string(&tool)?,
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

fn response_tool_to_claude(tool: openai::ResponseTool) -> Result<claude::Tool, TransformError> {
    match tool.type_ {
        openai::ToolType::Function => Ok(claude::Tool::Custom(claude::CustomTool {
            input_schema: schema_to_claude(tool.parameters)?,
            name: tool
                .name
                .ok_or_else(|| TransformError::shape("OpenAI tool", "name is missing"))?,
            type_: Some(claude::CustomToolType::Custom),
            description: tool.description,
            eager_input_streaming: None,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                defer_loading: tool.defer_loading,
                strict: tool.strict,
                rest: Default::default(),
                ..Default::default()
            },
            rest: tool.rest,
        })),
        _ => native::definitions::response_to_claude(tool),
    }
}

fn claude_tool_to_response(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
    match tool {
        claude::Tool::Custom(tool) => Ok(openai::ResponseTool {
            type_: openai::ToolType::Function,
            name: Some(tool.name),
            parameters: Some(schema_to_openai(tool.input_schema)?),
            strict: tool.common.strict,
            defer_loading: tool.common.defer_loading,
            description: tool.description,
            allowed_callers: callers_to_openai(tool.common.allowed_callers),
            rest: merge(tool.rest, tool.common.rest),
            ..empty_response_tool()
        }),
        other => native::definitions::claude_to_response(other),
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

pub(crate) fn empty_response_tool() -> openai::ResponseTool {
    openai::ResponseTool {
        type_: openai::ToolType::Function,
        name: None,
        parameters: None,
        strict: None,
        defer_loading: None,
        description: None,
        allowed_callers: None,
        vector_store_ids: None,
        filters: None,
        max_num_results: None,
        ranking_options: None,
        display_height: None,
        display_width: None,
        environment: None,
        max_uses: None,
        search_context_size: None,
        user_location: None,
        allowed_domains: None,
        blocked_domains: None,
        max_content_tokens: None,
        server_label: None,
        allowed_tools: None,
        authorization: None,
        connector_id: None,
        headers: None,
        require_approval: None,
        server_description: None,
        server_url: None,
        tunnel_id: None,
        container: None,
        action: None,
        background: None,
        input_fidelity: None,
        input_image_mask: None,
        model: None,
        moderation: None,
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: None,
        size: None,
        format: None,
        tools: None,
        execution: None,
        search_content_types: None,
        max_characters: None,
        rest: Default::default(),
    }
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

fn merge(
    mut left: serde_json::Map<String, serde_json::Value>,
    right: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    left.extend(right);
    left
}
