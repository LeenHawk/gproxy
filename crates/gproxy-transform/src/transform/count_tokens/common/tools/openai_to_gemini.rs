use crate::protocol::{gemini, openai};

use super::super::scalar::u32_to_i32;
use super::super::util::json_value;

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
