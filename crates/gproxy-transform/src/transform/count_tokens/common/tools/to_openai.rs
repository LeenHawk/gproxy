use crate::protocol::{claude, gemini, openai};

use super::super::scalar::i32_to_u32;
use super::super::util::{empty_string_to_none, json_object, json_value, non_empty_vec};

pub(in crate::transform::count_tokens) fn claude_tools_to_openai(
    tools: Option<Vec<claude::Tool>>,
    mcp_servers: Option<Vec<claude::McpServer>>,
) -> Option<Vec<openai::ResponseTool>> {
    let mut output = Vec::new();

    for tool in tools.into_iter().flatten() {
        match tool {
            claude::Tool::Custom(tool) => output.push(openai::ResponseTool::Function {
                name: tool.name,
                parameters: json_object(json_value(tool.input_schema)),
                strict: tool.common.strict,
                defer_loading: tool.common.defer_loading,
                description: tool.description,
                allowed_callers: None,
                extra: Default::default(),
            }),
            claude::Tool::WebSearch(_) => output.push(openai::ResponseTool::WebSearchPreview {
                search_content_types: None,
                search_context_size: None,
                user_location: None,
                extra: Default::default(),
            }),
            claude::Tool::WebFetch(_) => output.push(openai::ResponseTool::WebSearch {
                filters: None,
                search_context_size: None,
                user_location: None,
                extra: Default::default(),
            }),
            claude::Tool::Computer(_) => output.push(openai::ResponseTool::Computer {
                extra: Default::default(),
            }),
            claude::Tool::Command(
                claude::CommandTool::CodeExecution20250522(_)
                | claude::CommandTool::CodeExecution20250825(_)
                | claude::CommandTool::CodeExecution20260120(_)
                | claude::CommandTool::CodeExecution20260521(_),
            ) => output.push(openai::ResponseTool::CodeInterpreter {
                container: openai::CodeInterpreterContainer::Auto(
                    openai::CodeInterpreterAutoContainer {
                        type_: openai::CodeInterpreterContainerType::Auto,
                        file_ids: None,
                        memory_limit: None,
                        network_policy: None,
                        extra: Default::default(),
                    },
                ),
                allowed_callers: None,
                extra: Default::default(),
            }),
            claude::Tool::Command(_) => {}
            claude::Tool::McpToolset(toolset) => output.push(openai::ResponseTool::Mcp {
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
                extra: Default::default(),
            }),
            _ => {}
        }
    }

    output.extend(mcp_servers.into_iter().flatten().map(|server| {
        openai::ResponseTool::Mcp {
            server_label: server.name,
            allowed_tools: server
                .tool_configuration
                .and_then(|config| config.allowed_tools)
                .map(openai::McpAllowedTools::Names),
            authorization: server.authorization_token,
            connector_id: None,
            defer_loading: None,
            headers: None,
            require_approval: None,
            server_description: None,
            server_url: Some(server.url),
            tunnel_id: None,
            allowed_callers: None,
            extra: Default::default(),
        }
    }));

    non_empty_vec(output)
}

pub(in crate::transform::count_tokens) fn gemini_tools_to_openai(
    tools: Vec<gemini::Tool>,
) -> Option<Vec<openai::ResponseTool>> {
    let mut output = Vec::new();

    for tool in tools {
        output.extend(tool.function_declarations.into_iter().map(|function| {
            openai::ResponseTool::Function {
                name: function.name,
                parameters: function
                    .parameters_json_schema
                    .or_else(|| function.parameters.map(json_value))
                    .map(json_object)
                    .unwrap_or_default(),
                strict: Some(false),
                defer_loading: None,
                description: empty_string_to_none(function.description),
                allowed_callers: None,
                extra: Default::default(),
            }
        }));

        if let Some(file_search) = tool.file_search {
            output.push(openai::ResponseTool::FileSearch {
                vector_store_ids: file_search.file_search_store_names,
                filters: None,
                max_num_results: file_search.top_k.map(i32_to_u32),
                ranking_options: None,
                extra: Default::default(),
            });
        }
        if tool.google_search.is_some() || tool.google_search_retrieval.is_some() {
            output.push(openai::ResponseTool::WebSearchPreview {
                search_content_types: None,
                search_context_size: None,
                user_location: None,
                extra: Default::default(),
            });
        }
        if tool.code_execution.is_some() {
            output.push(openai::ResponseTool::CodeInterpreter {
                container: openai::CodeInterpreterContainer::Auto(
                    openai::CodeInterpreterAutoContainer {
                        type_: openai::CodeInterpreterContainerType::Auto,
                        file_ids: None,
                        memory_limit: None,
                        network_policy: None,
                        extra: Default::default(),
                    },
                ),
                allowed_callers: None,
                extra: Default::default(),
            });
        }
        if tool.computer_use.is_some() {
            output.push(openai::ResponseTool::Computer {
                extra: Default::default(),
            });
        }
        output.extend(tool.mcp_servers.into_iter().map(|server| {
            let transport = server.streamable_http_transport;
            openai::ResponseTool::Mcp {
                server_label: server.name.unwrap_or_default(),
                allowed_tools: None,
                authorization: None,
                connector_id: None,
                defer_loading: None,
                headers: transport
                    .as_ref()
                    .map(|transport| transport.headers.clone()),
                require_approval: None,
                server_description: None,
                server_url: transport.and_then(|transport| transport.url),
                tunnel_id: None,
                allowed_callers: None,
                extra: Default::default(),
            }
        }));
    }

    non_empty_vec(output)
}
