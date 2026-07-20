use crate::protocol::{claude, gemini, openai};

use super::super::util::{
    claude_json_schema, empty_string_to_none, json_object, json_value, non_empty_vec,
};

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
                    strict,
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
