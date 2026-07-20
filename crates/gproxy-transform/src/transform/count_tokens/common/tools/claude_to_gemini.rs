use crate::protocol::{claude, gemini};

use super::super::util::json_value;

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
