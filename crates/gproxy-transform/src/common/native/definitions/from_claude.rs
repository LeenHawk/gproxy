use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::tools::{callers_to_openai, merge};

pub(super) fn convert(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
    let tool = normalize(tool)?;
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
            claude::CommandTool::Memory20250818(tool) => memory(tool.common, tool.rest),
            claude::CommandTool::ToolSearchRegex(tool) => {
                tool_search(openai::ToolSearchExecution::Client, tool.common, tool.rest)
            }
            claude::CommandTool::ToolSearchBm25(tool) => {
                tool_search(openai::ToolSearchExecution::Server, tool.common, tool.rest)
            }
            other => fallback(claude::Tool::Command(other))?,
        },
        claude::Tool::TextEditor(editor) => text_editor(editor)?,
        claude::Tool::Computer(tool) => computer(tool)?,
        claude::Tool::WebSearch(search) => web_search(search)?,
        claude::Tool::WebFetch(fetch) => web_fetch(fetch)?,
        claude::Tool::McpToolset(toolset) => openai::ResponseTool::Mcp {
            server_label: toolset.mcp_server_name,
            allowed_tools: None,
            authorization: None,
            connector_id: None,
            defer_loading: None,
            headers: None,
            require_approval: None,
            server_description: None,
            server_url: None,
            tunnel_id: None,
            allowed_callers: None,
            rest: toolset.rest,
        },
        other => fallback(other)?,
    })
}

fn normalize(tool: claude::Tool) -> Result<claude::Tool, TransformError> {
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

fn shell(common: claude::ToolCommon, rest: openai::Rest) -> openai::ResponseTool {
    openai::ResponseTool::Shell {
        environment: None,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest: merge(rest, common.rest),
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
        other => return fallback(claude::Tool::TextEditor(other)),
    };
    Ok(openai::ResponseTool::ApplyPatch {
        allowed_callers: callers_to_openai(common.allowed_callers),
        max_characters,
        rest: merge(rest, common.rest),
    })
}

fn code_interpreter(
    callers: Option<Vec<claude::ToolCaller>>,
    rest: openai::Rest,
) -> openai::ResponseTool {
    openai::ResponseTool::CodeInterpreter {
        container: openai::CodeInterpreterContainer::Auto(openai::CodeInterpreterAutoContainer {
            type_: openai::CodeInterpreterContainerType::Auto,
            file_ids: None,
            memory_limit: None,
            network_policy: None,
            rest: Default::default(),
        }),
        allowed_callers: callers_to_openai(callers),
        rest,
    }
}

fn memory(common: claude::ToolCommon, rest: openai::Rest) -> openai::ResponseTool {
    openai::ResponseTool::Function {
        name: "memory".into(),
        parameters: openai::ResponseFunctionParameters::Null,
        strict: common
            .strict
            .map(openai::ResponseFunctionStrict::Value)
            .unwrap_or(openai::ResponseFunctionStrict::Absent),
        defer_loading: common.defer_loading,
        description: Some("Read or update persistent agent memory".into()),
        output_schema: None,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest: merge(rest, common.rest),
    }
}

fn tool_search(
    execution: openai::ToolSearchExecution,
    common: claude::ToolCommonWithoutInputExamples,
    rest: openai::Rest,
) -> openai::ResponseTool {
    openai::ResponseTool::ToolSearch {
        description: None,
        execution: Some(execution),
        parameters: None,
        rest: merge(rest, common.rest),
    }
}

fn computer(computer: claude::ComputerTool) -> Result<openai::ResponseTool, TransformError> {
    let (common, rest) = match computer {
        claude::ComputerTool::Computer20241022(tool) => (tool.common, tool.rest),
        claude::ComputerTool::Computer20250124(tool) => (tool.common, tool.rest),
        claude::ComputerTool::Computer20251124(tool) => (tool.common, tool.rest),
        other => return fallback(claude::Tool::Computer(other)),
    };
    Ok(openai::ResponseTool::Computer {
        rest: merge(rest, common.rest),
    })
}

fn web_search(search: claude::WebSearchTool) -> Result<openai::ResponseTool, TransformError> {
    let (params, common, rest) = match search {
        claude::WebSearchTool::WebSearch20250305(tool) => (tool.params, tool.common, tool.rest),
        claude::WebSearchTool::WebSearch20260209(tool) => (tool.params, tool.common, tool.rest),
        claude::WebSearchTool::WebSearch20260318(tool) => (tool.params, tool.common, tool.rest),
        other => return fallback(claude::Tool::WebSearch(other)),
    };
    let filters = if params.allowed_domains.is_some() || params.blocked_domains.is_some() {
        Some(openai::WebSearchFilters {
            allowed_domains: params.allowed_domains,
            blocked_domains: params.blocked_domains,
            rest: Default::default(),
        })
    } else {
        None
    };
    Ok(openai::ResponseTool::WebSearch {
        filters,
        max_uses: params.max_uses,
        search_context_size: None,
        user_location: params.user_location.map(location),
        rest: merge(merge(rest, common.rest), params.rest),
    })
}

fn web_fetch(fetch: claude::WebFetchTool) -> Result<openai::ResponseTool, TransformError> {
    let (params, common, rest) = match fetch {
        claude::WebFetchTool::WebFetch20250910(tool) => (tool.params, tool.common, tool.rest),
        claude::WebFetchTool::WebFetch20260209(tool) => (tool.params, tool.common, tool.rest),
        claude::WebFetchTool::WebFetch20260309(tool) => (tool.params, tool.common, tool.rest),
        claude::WebFetchTool::WebFetch20260318(tool) => (tool.params, tool.common, tool.rest),
        other => return fallback(claude::Tool::WebFetch(other)),
    };
    Ok(openai::ResponseTool::WebFetch {
        allowed_domains: params.allowed_domains,
        blocked_domains: params.blocked_domains,
        max_content_tokens: params.max_content_tokens,
        max_uses: params.max_uses,
        rest: merge(merge(rest, common.rest), params.rest),
    })
}

fn location(location: claude::UserLocation) -> openai::WebSearchUserLocation {
    openai::WebSearchUserLocation {
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        type_: Some(openai::ApproximateLocationType::Approximate),
        rest: location.rest,
    }
}

fn fallback(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
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
    let mut parameters = openai::JsonSchema::new();
    parameters.insert("type".into(), "object".into());
    Ok(openai::ResponseTool::Function {
        name,
        parameters: openai::ResponseFunctionParameters::Schema(parameters),
        strict: openai::ResponseFunctionStrict::Absent,
        defer_loading: None,
        description: None,
        output_schema: None,
        allowed_callers: None,
        rest,
    })
}
