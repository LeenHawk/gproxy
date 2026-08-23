use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::tools::{callers_to_claude, callers_to_openai, empty_response_tool};

pub(crate) fn claude_to_response(
    tool: claude::Tool,
) -> Result<openai::ResponseTool, TransformError> {
    let tool = normalize_claude_tool(tool)?;
    Ok(match tool {
        claude::Tool::Command(command) => match command {
            claude::CommandTool::Bash20241022(tool) => shell(tool.common, tool.rest),
            claude::CommandTool::Bash20250124(tool) => shell(tool.common, tool.rest),
            claude::CommandTool::CodeExecution20250522(tool) => {
                code_interpreter(tool.common.allowed_callers, tool.rest)
            }
            claude::CommandTool::CodeExecution20250825(tool) => {
                code_interpreter(tool.common.allowed_callers, tool.rest)
            }
            claude::CommandTool::CodeExecution20260120(tool) => {
                code_interpreter(tool.common.allowed_callers, tool.rest)
            }
            claude::CommandTool::CodeExecution20260521(tool) => {
                code_interpreter(tool.common.allowed_callers, tool.rest)
            }
            claude::CommandTool::ToolSearchRegex(tool) => tool_search(
                openai::ToolSearchExecution::Client,
                tool.common.allowed_callers,
                tool.rest,
            ),
            claude::CommandTool::ToolSearchBm25(tool) => tool_search(
                openai::ToolSearchExecution::Server,
                tool.common.allowed_callers,
                tool.rest,
            ),
            other => fallback_claude(claude::Tool::Command(other))?,
        },
        claude::Tool::TextEditor(editor) => text_editor(editor)?,
        claude::Tool::Computer(tool) => computer(tool)?,
        claude::Tool::WebSearch(search) => web_search(search)?,
        claude::Tool::McpToolset(toolset) => openai::ResponseTool {
            type_: openai::ToolType::Mcp,
            server_label: Some(toolset.mcp_server_name),
            rest: toolset.rest,
            ..empty_response_tool()
        },
        other => fallback_claude(other)?,
    })
}

fn normalize_claude_tool(tool: claude::Tool) -> Result<claude::Tool, TransformError> {
    let raw = match tool {
        claude::Tool::WebFetch(claude::WebFetchTool::Raw(raw))
        | claude::Tool::WebSearch(claude::WebSearchTool::Raw(raw))
        | claude::Tool::Computer(claude::ComputerTool::Raw(raw))
        | claude::Tool::TextEditor(claude::TextEditorTool::Raw(raw))
        | claude::Tool::Command(claude::CommandTool::Raw(raw))
        | claude::Tool::Unknown(raw) => raw,
        tool => return Ok(tool),
    };
    let type_ = raw
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(serde_json::Value::as_str);
    Ok(match type_ {
        Some(type_)
            if type_.starts_with("bash_")
                || type_.starts_with("code_execution_")
                || type_.starts_with("memory_")
                || type_.starts_with("tool_search_tool_") =>
        {
            claude::Tool::Command(serde_json::from_value(raw)?)
        }
        Some(type_) if type_.starts_with("text_editor_") => {
            claude::Tool::TextEditor(serde_json::from_value(raw)?)
        }
        Some(type_) if type_.starts_with("computer_") => {
            claude::Tool::Computer(serde_json::from_value(raw)?)
        }
        Some(type_) if type_.starts_with("web_search_") => {
            claude::Tool::WebSearch(serde_json::from_value(raw)?)
        }
        Some(type_) if type_.starts_with("web_fetch_") => {
            claude::Tool::WebFetch(serde_json::from_value(raw)?)
        }
        _ => claude::Tool::Unknown(raw),
    })
}

pub(crate) fn response_to_claude(
    tool: openai::ResponseTool,
) -> Result<claude::Tool, TransformError> {
    Ok(match tool.type_ {
        openai::ToolType::Shell | openai::ToolType::LocalShell => bash(tool),
        openai::ToolType::ApplyPatch => text_editor_from_response(tool),
        openai::ToolType::CodeInterpreter | openai::ToolType::CodeExecution => code_execution(tool),
        openai::ToolType::Computer
        | openai::ToolType::ComputerUse
        | openai::ToolType::ComputerUsePreview => computer_from_response(tool)?,
        openai::ToolType::WebSearch
        | openai::ToolType::WebSearch20250826
        | openai::ToolType::WebSearchPreview
        | openai::ToolType::WebSearchPreview20250311 => web_search_from_response(tool)?,
        openai::ToolType::ToolSearch => tool_search_from_response(tool),
        openai::ToolType::Mcp if tool.server_url.is_none() => {
            claude::Tool::McpToolset(claude::McpToolset {
                mcp_server_name: tool.server_label.ok_or_else(|| {
                    TransformError::shape("OpenAI MCP tool", "server_label is missing")
                })?,
                type_: claude::McpToolsetType::McpToolset,
                cache_control: None,
                configs: Default::default(),
                default_config: None,
                rest: tool.rest,
            })
        }
        _ => fallback_response(tool)?,
    })
}

fn shell(common: claude::ToolCommon, mut rest: openai::Rest) -> openai::ResponseTool {
    rest.extend(common.rest);
    openai::ResponseTool {
        type_: openai::ToolType::Shell,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest,
        ..empty_response_tool()
    }
}

fn text_editor(editor: claude::TextEditorTool) -> Result<openai::ResponseTool, TransformError> {
    let (common, max_characters, rest) = match editor {
        claude::TextEditorTool::TextEditor20241022(tool) => (tool.common, None, tool.rest),
        claude::TextEditorTool::TextEditor20250124(tool) => (tool.common, None, tool.rest),
        claude::TextEditorTool::TextEditor20250429(tool) => (tool.common, None, tool.rest),
        claude::TextEditorTool::TextEditor20250728(tool) => {
            (tool.common, tool.max_characters, tool.rest)
        }
        other => return fallback_claude(claude::Tool::TextEditor(other)),
    };
    let mut rest = rest;
    rest.extend(common.rest);
    Ok(openai::ResponseTool {
        type_: openai::ToolType::ApplyPatch,
        max_characters,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest,
        ..empty_response_tool()
    })
}

fn code_interpreter(
    callers: Option<Vec<claude::ToolCaller>>,
    rest: openai::Rest,
) -> openai::ResponseTool {
    openai::ResponseTool {
        type_: openai::ToolType::CodeInterpreter,
        allowed_callers: callers_to_openai(callers),
        container: Some(openai::CodeInterpreterContainer::Auto(
            openai::CodeInterpreterAutoContainer {
                type_: openai::CodeInterpreterContainerType::Auto,
                file_ids: None,
                memory_limit: None,
                network_policy: None,
                rest: Default::default(),
            },
        )),
        rest,
        ..empty_response_tool()
    }
}

fn tool_search(
    execution: openai::ToolSearchExecution,
    callers: Option<Vec<claude::ToolCaller>>,
    rest: openai::Rest,
) -> openai::ResponseTool {
    openai::ResponseTool {
        type_: openai::ToolType::ToolSearch,
        execution: Some(execution),
        allowed_callers: callers_to_openai(callers),
        rest,
        ..empty_response_tool()
    }
}

fn computer(computer: claude::ComputerTool) -> Result<openai::ResponseTool, TransformError> {
    let fallback = computer.clone();
    let (width, height, common, rest) = match computer {
        claude::ComputerTool::Computer20241022(tool) => (
            tool.display_width_px,
            tool.display_height_px,
            tool.common,
            tool.rest,
        ),
        claude::ComputerTool::Computer20250124(tool) => (
            tool.display_width_px,
            tool.display_height_px,
            tool.common,
            tool.rest,
        ),
        claude::ComputerTool::Computer20251124(tool) => (
            tool.display_width_px,
            tool.display_height_px,
            tool.common,
            tool.rest,
        ),
        other => return fallback_claude(claude::Tool::Computer(other)),
    };
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return fallback_claude(claude::Tool::Computer(fallback));
    };
    Ok(openai::ResponseTool {
        type_: openai::ToolType::ComputerUsePreview,
        display_width: Some(width),
        display_height: Some(height),
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest,
        ..empty_response_tool()
    })
}

fn web_search(search: claude::WebSearchTool) -> Result<openai::ResponseTool, TransformError> {
    let (params, common, rest) = match search {
        claude::WebSearchTool::WebSearch20250305(tool) => (tool.params, tool.common, tool.rest),
        claude::WebSearchTool::WebSearch20260209(tool) => (tool.params, tool.common, tool.rest),
        claude::WebSearchTool::WebSearch20260318(tool) => (tool.params, tool.common, tool.rest),
        other => return fallback_claude(claude::Tool::WebSearch(other)),
    };
    let filters = if params.allowed_domains.is_some() || params.blocked_domains.is_some() {
        let mut value = serde_json::Map::new();
        if let Some(domains) = params.allowed_domains {
            value.insert("allowed_domains".into(), serde_json::to_value(domains)?);
        }
        if let Some(domains) = params.blocked_domains {
            value.insert("blocked_domains".into(), serde_json::to_value(domains)?);
        }
        Some(serde_json::Value::Object(value))
    } else {
        None
    };
    Ok(openai::ResponseTool {
        type_: openai::ToolType::WebSearch,
        filters,
        max_uses: params.max_uses,
        user_location: params.user_location.map(serde_json::to_value).transpose()?,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest,
        ..empty_response_tool()
    })
}

fn bash(tool: openai::ResponseTool) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                rest: Default::default(),
                ..Default::default()
            },
            rest: tool.rest,
        },
    ))
}

fn text_editor_from_response(tool: openai::ResponseTool) -> claude::Tool {
    claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(
        claude::TextEditorTool20250728 {
            name: claude::StrReplaceBasedEditToolName::StrReplaceBasedEditTool,
            type_: claude::TextEditorTool20250728Type::TextEditor20250728,
            max_characters: tool.max_characters,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                rest: Default::default(),
                ..Default::default()
            },
            rest: tool.rest,
        },
    ))
}

fn code_execution(tool: openai::ResponseTool) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::CodeExecution20260120(
        claude::CodeExecutionTool20260120 {
            name: claude::CodeExecutionToolName::CodeExecution,
            type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
            common: claude::ToolCommonWithoutInputExamples {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                ..Default::default()
            },
            rest: tool.rest,
        },
    ))
}

fn computer_from_response(tool: openai::ResponseTool) -> Result<claude::Tool, TransformError> {
    let (Some(width), Some(height)) = (tool.display_width, tool.display_height) else {
        return fallback_response(tool);
    };
    Ok(claude::Tool::Computer(
        claude::ComputerTool::Computer20250124(claude::ComputerTool20250124 {
            display_height_px: u64::from(height),
            display_width_px: u64::from(width),
            name: claude::ComputerToolName::Computer,
            type_: claude::ComputerTool20250124Type::Computer20250124,
            display_number: None,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                ..Default::default()
            },
            rest: tool.rest,
        }),
    ))
}

fn web_search_from_response(tool: openai::ResponseTool) -> Result<claude::Tool, TransformError> {
    let (allowed_domains, blocked_domains) = match tool.filters {
        Some(serde_json::Value::Object(mut filters)) => (
            filters
                .remove("allowed_domains")
                .map(serde_json::from_value)
                .transpose()?,
            filters
                .remove("blocked_domains")
                .map(serde_json::from_value)
                .transpose()?,
        ),
        _ => (None, None),
    };
    Ok(claude::Tool::WebSearch(
        claude::WebSearchTool::WebSearch20260209(claude::WebSearchTool20260209 {
            name: claude::WebSearchToolName::WebSearch,
            type_: claude::WebSearchTool20260209Type::WebSearch20260209,
            params: claude::WebSearchToolParams {
                allowed_domains,
                blocked_domains,
                max_uses: tool.max_uses,
                user_location: tool.user_location.map(serde_json::from_value).transpose()?,
                rest: Default::default(),
            },
            common: claude::ToolCommonWithoutInputExamples {
                allowed_callers: callers_to_claude(tool.allowed_callers),
                ..Default::default()
            },
            rest: tool.rest,
        }),
    ))
}

fn tool_search_from_response(tool: openai::ResponseTool) -> claude::Tool {
    let common = claude::ToolCommonWithoutInputExamples {
        allowed_callers: callers_to_claude(tool.allowed_callers),
        ..Default::default()
    };
    if matches!(tool.execution, Some(openai::ToolSearchExecution::Client)) {
        claude::Tool::Command(claude::CommandTool::ToolSearchRegex(
            claude::ToolSearchRegexTool {
                name: claude::ToolSearchRegexToolName::ToolSearchRegex,
                type_: claude::ToolSearchRegexToolType::ToolSearchRegex,
                common,
                rest: tool.rest,
            },
        ))
    } else {
        claude::Tool::Command(claude::CommandTool::ToolSearchBm25(
            claude::ToolSearchBm25Tool {
                name: claude::ToolSearchBm25ToolName::ToolSearchBm25,
                type_: claude::ToolSearchBm25ToolType::ToolSearchBm25,
                common,
                rest: tool.rest,
            },
        ))
    }
}

fn fallback_claude(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
    let raw = serde_json::to_value(&tool)?;
    let object = raw.as_object();
    let name = object
        .and_then(|object| object.get("name"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            object
                .and_then(|object| object.get("type"))
                .and_then(serde_json::Value::as_str)
        })
        .ok_or_else(|| {
            TransformError::shape(
                "Claude native tool fallback",
                "both name and type are missing",
            )
        })?
        .to_owned();
    let mut rest = openai::Rest::new();
    rest.insert("source_tool_definition".into(), raw);
    let mut schema = openai::JsonSchema::new();
    schema.insert("type".into(), "object".into());
    Ok(openai::ResponseTool {
        type_: openai::ToolType::Function,
        name: Some(name),
        parameters: Some(schema),
        rest,
        ..empty_response_tool()
    })
}

fn fallback_response(tool: openai::ResponseTool) -> Result<claude::Tool, TransformError> {
    let raw = serde_json::to_value(&tool)?;
    let name = tool.name.clone().ok_or_else(|| {
        TransformError::shape(
            "OpenAI native tool fallback",
            "a callable name is not declared",
        )
    })?;
    let mut rest = claude::JsonObject::new();
    rest.insert("openai_native_tool".into(), raw);
    Ok(claude::Tool::Custom(claude::CustomTool {
        input_schema: claude::JsonSchema {
            type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
            properties: Default::default(),
            required: Vec::new(),
            rest: Default::default(),
        },
        name,
        type_: Some(claude::CustomToolType::Custom),
        description: tool.description,
        eager_input_streaming: None,
        common: claude::ToolCommon {
            allowed_callers: callers_to_claude(tool.allowed_callers),
            defer_loading: tool.defer_loading,
            strict: tool.strict,
            ..Default::default()
        },
        rest,
    }))
}
