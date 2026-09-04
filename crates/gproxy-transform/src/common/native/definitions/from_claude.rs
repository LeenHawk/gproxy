use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::tools::callers_to_openai;

pub(super) fn convert(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
    let tool = normalize(tool)?;
    Ok(match tool {
        claude::Tool::Custom(tool) => custom(tool)?,
        claude::Tool::Command(command) => match command {
            claude::CommandTool::Bash20241022(tool) => shell(tool.common),
            claude::CommandTool::Bash20250124(tool) => shell(tool.common),
            claude::CommandTool::CodeExecution20250522(tool) => {
                code_interpreter(tool.common.allowed_callers)
            }
            claude::CommandTool::CodeExecution20250825(tool) => {
                code_interpreter(tool.common.allowed_callers)
            }
            claude::CommandTool::CodeExecution20260120(tool) => {
                code_interpreter(tool.common.allowed_callers)
            }
            claude::CommandTool::CodeExecution20260521(tool) => {
                code_interpreter(tool.common.allowed_callers)
            }
            claude::CommandTool::Memory20250818(tool) => memory(tool.common),
            claude::CommandTool::ToolSearchRegex(tool) => {
                tool_search(openai::ToolSearchExecution::Client, tool.common)
            }
            claude::CommandTool::ToolSearchBm25(tool) => {
                tool_search(openai::ToolSearchExecution::Server, tool.common)
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
            rest: Default::default(),
        },
        other => fallback(other)?,
    })
}

fn custom(tool: claude::CustomTool) -> Result<openai::ResponseTool, TransformError> {
    let parameters = serde_json::to_value(tool.input_schema)?
        .as_object()
        .cloned()
        .ok_or_else(|| TransformError::shape("Claude custom tool", "schema must be an object"))?;
    Ok(openai::ResponseTool::Function {
        name: tool.name,
        parameters: openai::ResponseFunctionParameters::Schema(parameters),
        strict: tool
            .common
            .strict
            .map(openai::ResponseFunctionStrict::Value)
            .unwrap_or(openai::ResponseFunctionStrict::Absent),
        defer_loading: tool.common.defer_loading,
        description: tool.description,
        output_schema: None,
        allowed_callers: callers_to_openai(tool.common.allowed_callers),
        async_: None,
        rest: Default::default(),
    })
}

fn normalize(tool: claude::Tool) -> Result<claude::Tool, TransformError> {
    match tool {
        claude::Tool::WebFetch(claude::WebFetchTool::Raw(raw))
        | claude::Tool::WebSearch(claude::WebSearchTool::Raw(raw))
        | claude::Tool::Computer(claude::ComputerTool::Raw(raw))
        | claude::Tool::TextEditor(claude::TextEditorTool::Raw(raw))
        | claude::Tool::Command(claude::CommandTool::Raw(raw))
        | claude::Tool::Unknown(raw) => {
            Err(TransformError::unsupported("Claude tool", raw.to_string()))
        }
        tool => Ok(tool),
    }
}

fn shell(common: claude::ToolCommon) -> openai::ResponseTool {
    openai::ResponseTool::Shell {
        environment: None,
        allowed_callers: callers_to_openai(common.allowed_callers),
        rest: Default::default(),
    }
}

fn text_editor(editor: claude::TextEditorTool) -> Result<openai::ResponseTool, TransformError> {
    let (common, max_characters) = match editor {
        claude::TextEditorTool::TextEditor20241022(tool) => (tool.common, None),
        claude::TextEditorTool::TextEditor20250124(tool) => (tool.common, None),
        claude::TextEditorTool::TextEditor20250429(tool) => (tool.common, None),
        claude::TextEditorTool::TextEditor20250728(tool) => (tool.common, tool.max_characters),
        other => return fallback(claude::Tool::TextEditor(other)),
    };
    Ok(openai::ResponseTool::ApplyPatch {
        allowed_callers: callers_to_openai(common.allowed_callers),
        max_characters,
        rest: Default::default(),
    })
}

fn code_interpreter(callers: Option<Vec<claude::ToolCaller>>) -> openai::ResponseTool {
    openai::ResponseTool::CodeInterpreter {
        container: openai::CodeInterpreterContainer::Auto(crate::wire!(
            openai::CodeInterpreterAutoContainer {
                type_: openai::CodeInterpreterContainerType::Auto,
                file_ids: None,
                memory_limit: None,
                network_policy: None,
                rest: Default::default(),
            }
        )),
        allowed_callers: callers_to_openai(callers),
        rest: Default::default(),
    }
}

fn memory(common: claude::ToolCommon) -> openai::ResponseTool {
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
        async_: None,
        rest: Default::default(),
    }
}

fn tool_search(
    execution: openai::ToolSearchExecution,
    _common: claude::ToolCommonWithoutInputExamples,
) -> openai::ResponseTool {
    openai::ResponseTool::ToolSearch {
        description: None,
        execution: Some(execution),
        parameters: None,
        rest: Default::default(),
    }
}

fn computer(computer: claude::ComputerTool) -> Result<openai::ResponseTool, TransformError> {
    match computer {
        claude::ComputerTool::Computer20241022(_)
        | claude::ComputerTool::Computer20250124(_)
        | claude::ComputerTool::Computer20251124(_) => {}
        other => return fallback(claude::Tool::Computer(other)),
    }
    Ok(openai::ResponseTool::Computer {
        rest: Default::default(),
    })
}

fn web_search(search: claude::WebSearchTool) -> Result<openai::ResponseTool, TransformError> {
    let params = match search {
        claude::WebSearchTool::WebSearch20250305(tool) => tool.params,
        claude::WebSearchTool::WebSearch20260209(tool) => tool.params,
        claude::WebSearchTool::WebSearch20260318(tool) => tool.params,
        other => return fallback(claude::Tool::WebSearch(other)),
    };
    let filters = if params.allowed_domains.is_some() || params.blocked_domains.is_some() {
        Some(crate::wire!(openai::WebSearchFilters {
            allowed_domains: params.allowed_domains,
            blocked_domains: params.blocked_domains,
            rest: Default::default(),
        }))
    } else {
        None
    };
    Ok(openai::ResponseTool::WebSearch {
        filters,
        max_uses: params.max_uses,
        search_context_size: None,
        user_location: params.user_location.map(location),
        rest: Default::default(),
    })
}

fn web_fetch(fetch: claude::WebFetchTool) -> Result<openai::ResponseTool, TransformError> {
    let params = match fetch {
        claude::WebFetchTool::WebFetch20250910(tool) => tool.params,
        claude::WebFetchTool::WebFetch20260209(tool) => tool.params,
        claude::WebFetchTool::WebFetch20260309(tool) => tool.params,
        claude::WebFetchTool::WebFetch20260318(tool) => tool.params,
        other => return fallback(claude::Tool::WebFetch(other)),
    };
    Ok(openai::ResponseTool::WebFetch {
        allowed_domains: params.allowed_domains,
        blocked_domains: params.blocked_domains,
        max_content_tokens: params.max_content_tokens,
        max_uses: params.max_uses,
        rest: Default::default(),
    })
}

fn location(location: claude::UserLocation) -> openai::WebSearchUserLocation {
    crate::wire!(openai::WebSearchUserLocation {
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        type_: Some(openai::ApproximateLocationType::Approximate),
        rest: Default::default(),
    })
}

fn fallback(tool: claude::Tool) -> Result<openai::ResponseTool, TransformError> {
    Err(TransformError::unsupported(
        "Claude native tool",
        serde_json::to_string(&tool)?,
    ))
}
