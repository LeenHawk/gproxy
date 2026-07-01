use crate::protocol::{claude, gemini, openai};

use super::scalar::{i32_to_u32, u32_to_i32};
use super::util::{
    claude_json_schema, empty_string_to_none, json_object, json_value, non_empty_vec,
};

pub(in crate::transform::count_tokens) fn openai_tools_to_gemini(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Vec<gemini::Tool> {
    let mut declarations = Vec::new();
    let mut gemini_tools = Vec::new();

    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ResponseTool::Function {
                name,
                parameters,
                description,
                ..
            } => declarations.push(gemini::FunctionDeclaration {
                name,
                description: description.unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema: Some(json_value(parameters)),
                response: None,
                response_json_schema: None,
                extra: Default::default(),
            }),
            openai::ResponseTool::Namespace { tools, .. } => {
                declarations.extend(tools.into_iter().filter_map(openai_namespace_function));
            }
            openai::ResponseTool::FileSearch {
                vector_store_ids,
                max_num_results,
                ..
            } => gemini_tools.push(gemini::Tool {
                file_search: Some(gemini::FileSearch {
                    file_search_store_names: vector_store_ids,
                    metadata_filter: None,
                    top_k: max_num_results.map(u32_to_i32),
                    extra: Default::default(),
                }),
                ..Default::default()
            }),
            openai::ResponseTool::WebSearch { .. }
            | openai::ResponseTool::WebSearch20250826 { .. }
            | openai::ResponseTool::WebSearchPreview { .. }
            | openai::ResponseTool::WebSearchPreview20250311 { .. } => {
                gemini_tools.push(gemini::Tool {
                    google_search: Some(gemini::GoogleSearch::default()),
                    ..Default::default()
                });
            }
            openai::ResponseTool::CodeInterpreter { .. } => gemini_tools.push(gemini::Tool {
                code_execution: Some(gemini::CodeExecution::default()),
                ..Default::default()
            }),
            openai::ResponseTool::Computer { .. }
            | openai::ResponseTool::ComputerUsePreview { .. } => gemini_tools.push(gemini::Tool {
                computer_use: Some(gemini::ComputerUse::default()),
                ..Default::default()
            }),
            openai::ResponseTool::Mcp {
                server_label,
                server_url,
                headers,
                ..
            } => gemini_tools.push(gemini::Tool {
                mcp_servers: vec![gemini::McpServer {
                    name: Some(server_label),
                    streamable_http_transport: server_url.map(|url| {
                        gemini::StreamableHttpTransport {
                            url: Some(url),
                            headers: headers.unwrap_or_default(),
                            timeout: None,
                            sse_read_timeout: None,
                            terminate_on_close: None,
                            extra: Default::default(),
                        }
                    }),
                    extra: Default::default(),
                }],
                ..Default::default()
            }),
            _ => {}
        }
    }

    if !declarations.is_empty() {
        gemini_tools.insert(
            0,
            gemini::Tool {
                function_declarations: declarations,
                ..Default::default()
            },
        );
    }

    gemini_tools
}

fn openai_namespace_function(
    tool: openai::ResponseNamespaceTool,
) -> Option<gemini::FunctionDeclaration> {
    match tool {
        openai::ResponseNamespaceTool::Function {
            name,
            description,
            parameters,
            ..
        } => Some(gemini::FunctionDeclaration {
            name,
            description: description.unwrap_or_default(),
            behavior: None,
            parameters: None,
            parameters_json_schema: parameters,
            response: None,
            response_json_schema: None,
            extra: Default::default(),
        }),
        _ => None,
    }
}

pub(in crate::transform::count_tokens) fn claude_tools_to_gemini(
    tools: Option<Vec<claude::Tool>>,
    mcp_servers: Option<Vec<claude::McpServer>>,
) -> Vec<gemini::Tool> {
    let mut declarations = Vec::new();
    let mut gemini_tools = Vec::new();

    for tool in tools.into_iter().flatten() {
        match tool {
            claude::Tool::Custom(tool) => declarations.push(gemini::FunctionDeclaration {
                name: tool.name,
                description: tool.description.unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema: Some(json_value(tool.input_schema)),
                response: None,
                response_json_schema: None,
                extra: Default::default(),
            }),
            claude::Tool::WebSearch(_) => gemini_tools.push(gemini::Tool {
                google_search: Some(gemini::GoogleSearch::default()),
                ..Default::default()
            }),
            claude::Tool::WebFetch(_) => gemini_tools.push(gemini::Tool {
                url_context: Some(gemini::UrlContext::default()),
                ..Default::default()
            }),
            claude::Tool::Computer(_) => gemini_tools.push(gemini::Tool {
                computer_use: Some(gemini::ComputerUse::default()),
                ..Default::default()
            }),
            claude::Tool::Command(
                claude::CommandTool::CodeExecution20250522(_)
                | claude::CommandTool::CodeExecution20250825(_)
                | claude::CommandTool::CodeExecution20260120(_)
                | claude::CommandTool::CodeExecution20260521(_),
            ) => gemini_tools.push(gemini::Tool {
                code_execution: Some(gemini::CodeExecution::default()),
                ..Default::default()
            }),
            claude::Tool::Command(_) => {}
            claude::Tool::McpToolset(toolset) => gemini_tools.push(gemini::Tool {
                mcp_servers: vec![gemini::McpServer {
                    name: Some(toolset.mcp_server_name),
                    streamable_http_transport: None,
                    extra: Default::default(),
                }],
                ..Default::default()
            }),
            _ => {}
        }
    }

    let mcp_servers = mcp_servers
        .into_iter()
        .flatten()
        .map(|server| gemini::McpServer {
            name: Some(server.name),
            streamable_http_transport: Some(gemini::StreamableHttpTransport {
                url: Some(server.url),
                headers: Default::default(),
                timeout: None,
                sse_read_timeout: None,
                terminate_on_close: None,
                extra: Default::default(),
            }),
            extra: Default::default(),
        })
        .collect::<Vec<_>>();
    if !mcp_servers.is_empty() {
        gemini_tools.push(gemini::Tool {
            mcp_servers,
            ..Default::default()
        });
    }

    if !declarations.is_empty() {
        gemini_tools.insert(
            0,
            gemini::Tool {
                function_declarations: declarations,
                ..Default::default()
            },
        );
    }

    gemini_tools
}

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
                strict: tool.common.strict.unwrap_or_default(),
                defer_loading: tool.common.defer_loading,
                description: tool.description,
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
                strict: false,
                defer_loading: None,
                description: empty_string_to_none(function.description),
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
                extra: Default::default(),
            }
        }));
    }

    non_empty_vec(output)
}

pub(in crate::transform::count_tokens) struct ClaudeToolParts {
    pub tools: Option<Vec<claude::Tool>>,
    pub mcp_servers: Option<Vec<claude::McpServer>>,
}

pub(in crate::transform::count_tokens) fn gemini_tools_to_claude(
    tools: Vec<gemini::Tool>,
) -> ClaudeToolParts {
    let mut output_tools = Vec::new();
    let mut mcp_servers = Vec::new();

    for tool in tools {
        output_tools.extend(tool.function_declarations.into_iter().map(|function| {
            claude::Tool::Custom(claude::CustomTool {
                input_schema: function
                    .parameters_json_schema
                    .or_else(|| function.parameters.map(json_value))
                    .map(json_object)
                    .map(claude_json_schema)
                    .unwrap_or_else(|| claude_json_schema(Default::default())),
                name: function.name,
                type_: Some(claude::CustomToolType::Custom),
                description: empty_string_to_none(function.description),
                eager_input_streaming: None,
                common: Default::default(),
            })
        }));

        if tool.google_search.is_some() || tool.google_search_retrieval.is_some() {
            output_tools.push(claude::Tool::WebSearch(
                claude::WebSearchTool::WebSearch20260209(claude::WebSearchTool20260209 {
                    name: claude::WebSearchToolName::WebSearch,
                    type_: claude::WebSearchTool20260209Type::WebSearch20260209,
                    params: claude::WebSearchToolParams {
                        allowed_domains: None,
                        blocked_domains: None,
                        max_uses: None,
                        user_location: None,
                    },
                    common: Default::default(),
                }),
            ));
        }

        if tool.url_context.is_some() {
            output_tools.push(claude::Tool::WebFetch(
                claude::WebFetchTool::WebFetch20260309(claude::WebFetchTool20260309 {
                    name: claude::WebFetchToolName::WebFetch,
                    type_: claude::WebFetchTool20260309Type::WebFetch20260309,
                    params: claude::WebFetchToolParams {
                        allowed_domains: None,
                        blocked_domains: None,
                        citations: None,
                        max_content_tokens: None,
                        max_uses: None,
                    },
                    use_cache: None,
                    common: Default::default(),
                }),
            ));
        }

        if tool.code_execution.is_some() {
            output_tools.push(claude::Tool::Command(
                claude::CommandTool::CodeExecution20260120(claude::CodeExecutionTool20260120 {
                    name: claude::CodeExecutionToolName::CodeExecution,
                    type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
                    common: Default::default(),
                }),
            ));
        }

        for server in tool.mcp_servers {
            if let Some(server) = gemini_mcp_server_to_claude_server(server.clone()) {
                mcp_servers.push(server);
            } else if let Some(toolset) = gemini_mcp_server_to_claude_toolset(server) {
                output_tools.push(claude::Tool::McpToolset(toolset));
            }
        }
    }

    ClaudeToolParts {
        tools: non_empty_vec(output_tools),
        mcp_servers: non_empty_vec(mcp_servers),
    }
}

fn gemini_mcp_server_to_claude_server(server: gemini::McpServer) -> Option<claude::McpServer> {
    let transport = server.streamable_http_transport?;
    Some(claude::McpServer {
        name: server.name.unwrap_or_default(),
        type_: claude::McpServerType::Known(claude::McpServerTypeKnown::Url),
        url: transport.url?,
        authorization_token: None,
        tool_configuration: None,
        extra: Default::default(),
    })
}

fn gemini_mcp_server_to_claude_toolset(server: gemini::McpServer) -> Option<claude::McpToolset> {
    Some(claude::McpToolset {
        mcp_server_name: server.name?,
        type_: claude::McpToolsetType::McpToolset,
        cache_control: None,
        configs: Default::default(),
        default_config: None,
    })
}

pub(in crate::transform::count_tokens) fn openai_tools_to_claude(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Option<Vec<claude::Tool>> {
    let mut output = Vec::new();

    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ResponseTool::Function {
                name,
                parameters,
                strict,
                defer_loading,
                description,
                ..
            } => output.push(claude::Tool::Custom(claude::CustomTool {
                input_schema: claude_json_schema(parameters),
                name,
                type_: Some(claude::CustomToolType::Custom),
                description,
                eager_input_streaming: None,
                common: claude::ToolCommon {
                    defer_loading,
                    strict: Some(strict),
                    ..Default::default()
                },
            })),
            openai::ResponseTool::Namespace { tools, .. } => {
                output.extend(
                    tools
                        .into_iter()
                        .filter_map(openai_namespace_tool_to_claude),
                );
            }
            _ => {}
        }
    }

    non_empty_vec(output)
}

pub(in crate::transform::count_tokens) fn openai_mcp_servers_to_claude(
    tools: Option<&[openai::ResponseTool]>,
) -> Option<Vec<claude::McpServer>> {
    let output = tools
        .into_iter()
        .flatten()
        .filter_map(|tool| match tool {
            openai::ResponseTool::Mcp {
                server_label,
                allowed_tools,
                authorization,
                server_url: Some(server_url),
                ..
            } => Some(claude::McpServer {
                name: server_label.clone(),
                type_: claude::McpServerType::Known(claude::McpServerTypeKnown::Url),
                url: server_url.clone(),
                authorization_token: authorization.clone(),
                tool_configuration: allowed_tools
                    .as_ref()
                    .and_then(openai_mcp_allowed_tools_to_claude),
                extra: Default::default(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    non_empty_vec(output)
}

fn openai_mcp_allowed_tools_to_claude(
    allowed_tools: &openai::McpAllowedTools,
) -> Option<claude::McpToolConfiguration> {
    let openai::McpAllowedTools::Names(names) = allowed_tools else {
        return None;
    };
    Some(claude::McpToolConfiguration {
        allowed_tools: Some(names.clone()),
        enabled: None,
        extra: Default::default(),
    })
}

fn openai_namespace_tool_to_claude(tool: openai::ResponseNamespaceTool) -> Option<claude::Tool> {
    match tool {
        openai::ResponseNamespaceTool::Function {
            name,
            description,
            parameters,
            strict,
            defer_loading,
            ..
        } => Some(claude::Tool::Custom(claude::CustomTool {
            input_schema: parameters
                .map(json_object)
                .map(claude_json_schema)
                .unwrap_or_else(|| claude_json_schema(Default::default())),
            name,
            type_: Some(claude::CustomToolType::Custom),
            description,
            eager_input_streaming: None,
            common: claude::ToolCommon {
                defer_loading,
                strict,
                ..Default::default()
            },
        })),
        _ => None,
    }
}
