use serde_json::Value;

use crate::protocol::{gemini, openai};

pub(super) fn response_tools_to_gemini(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Vec<gemini::Tool> {
    let mut declarations = Vec::new();
    let mut output = Vec::new();
    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ResponseTool::Function {
                name,
                parameters,
                description,
                ..
            } => {
                declarations.push(function(
                    name,
                    description,
                    Some(Value::Object(parameters.into_iter().collect())),
                ));
            }
            openai::ResponseTool::Custom {
                name, description, ..
            } => {
                declarations.push(function(name, description, None));
            }
            openai::ResponseTool::Namespace { tools, .. } => {
                declarations.extend(tools.into_iter().filter_map(namespace_function))
            }
            openai::ResponseTool::FileSearch {
                vector_store_ids,
                max_num_results,
                ..
            } => {
                output.push(crate::protocol::wire!(gemini::Tool {
                    file_search: Some(crate::protocol::wire!(gemini::FileSearch {
                        file_search_store_names: vector_store_ids,
                        metadata_filter: None,
                        top_k: max_num_results.map(u32_to_i32),
                        extra: Default::default(),
                    })),
                    ..Default::default()
                }));
            }
            openai::ResponseTool::WebSearch { .. }
            | openai::ResponseTool::WebSearch20250826 { .. } => {
                output.push(crate::protocol::wire!(gemini::Tool {
                    google_search: Some(gemini::GoogleSearch::default()),
                    ..Default::default()
                }))
            }
            openai::ResponseTool::WebSearchPreview {
                search_content_types,
                ..
            }
            | openai::ResponseTool::WebSearchPreview20250311 {
                search_content_types,
                ..
            } => output.push(crate::protocol::wire!(gemini::Tool {
                google_search: Some(crate::protocol::wire!(gemini::GoogleSearch {
                    search_types: response_search_types_to_gemini(search_content_types),
                    ..Default::default()
                })),
                ..Default::default()
            })),
            openai::ResponseTool::CodeInterpreter { .. }
            | openai::ResponseTool::CodeExecution { .. } => {
                output.push(crate::protocol::wire!(gemini::Tool {
                    code_execution: Some(gemini::CodeExecution::default()),
                    ..Default::default()
                }));
            }
            openai::ResponseTool::Computer { .. }
            | openai::ResponseTool::ComputerUsePreview { .. } => {
                output.push(crate::protocol::wire!(gemini::Tool {
                    computer_use: Some(gemini::ComputerUse::default()),
                    ..Default::default()
                }));
            }
            openai::ResponseTool::Mcp {
                server_label,
                server_url,
                headers,
                ..
            } => output.push(crate::protocol::wire!(gemini::Tool {
                mcp_servers: vec![crate::protocol::wire!(gemini::McpServer {
                    name: Some(server_label),
                    streamable_http_transport: server_url.map(|url| {
                        crate::protocol::wire!(gemini::StreamableHttpTransport {
                            url: Some(url),
                            headers: headers.unwrap_or_default(),
                            timeout: None,
                            sse_read_timeout: None,
                            terminate_on_close: None,
                            extra: Default::default(),
                        })
                    }),
                    extra: Default::default(),
                })],
                ..Default::default()
            })),
            _ => {}
        }
    }
    if !declarations.is_empty() {
        output.insert(
            0,
            crate::protocol::wire!(gemini::Tool {
                function_declarations: declarations,
                ..Default::default()
            }),
        );
    }
    output
}

pub(super) fn response_tool_choice_to_gemini(
    choice: Option<openai::ResponseToolChoice>,
) -> Option<gemini::ToolConfig> {
    let (mode, names) = match choice? {
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None) => {
            (gemini::FunctionCallingModeKnown::None, Vec::new())
        }
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Auto) => {
            (gemini::FunctionCallingModeKnown::Auto, Vec::new())
        }
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Required) => {
            (gemini::FunctionCallingModeKnown::Any, Vec::new())
        }
        openai::ResponseToolChoice::Function(choice) => {
            (gemini::FunctionCallingModeKnown::Any, vec![choice.name])
        }
        openai::ResponseToolChoice::Custom(choice) => {
            (gemini::FunctionCallingModeKnown::Any, vec![choice.name])
        }
        openai::ResponseToolChoice::Allowed(choice) => {
            let mode = match choice.mode {
                openai::AllowedToolsMode::Auto => gemini::FunctionCallingModeKnown::Auto,
                openai::AllowedToolsMode::Required => gemini::FunctionCallingModeKnown::Any,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            let names = choice
                .tools
                .into_iter()
                .filter_map(|tool| match tool {
                    openai::ResponseAllowedTool::Function { name, .. }
                    | openai::ResponseAllowedTool::Custom { name, .. } => Some(name),
                    _ => None,
                })
                .collect();
            (mode, names)
        }
        _ => return None,
    };
    Some(crate::protocol::wire!(gemini::ToolConfig {
        function_calling_config: Some(crate::protocol::wire!(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(mode)),
            allowed_function_names: names,
            extra: Default::default(),
        })),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        extra: Default::default(),
    }))
}

fn function(
    name: String,
    description: Option<String>,
    schema: Option<Value>,
) -> gemini::FunctionDeclaration {
    crate::protocol::wire!(gemini::FunctionDeclaration {
        name,
        description: description.unwrap_or_default(),
        behavior: None,
        parameters: None,
        parameters_json_schema: schema,
        response: None,
        response_json_schema: None,
        extra: Default::default(),
    })
}

fn namespace_function(tool: openai::ResponseNamespaceTool) -> Option<gemini::FunctionDeclaration> {
    match tool {
        openai::ResponseNamespaceTool::Function {
            name,
            description,
            parameters,
            ..
        } => Some(function(name, description, parameters)),
        openai::ResponseNamespaceTool::Custom {
            name, description, ..
        } => Some(function(name, description, None)),
        _ => None,
    }
}

fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn response_search_types_to_gemini(
    content_types: Option<Vec<openai::SearchContentType>>,
) -> Option<gemini::SearchTypes> {
    let content_types = content_types?;
    let mut web_search = None;
    let mut image_search = None;
    for content_type in content_types {
        match content_type {
            openai::SearchContentType::Text => web_search = Some(gemini::WebSearch::default()),
            openai::SearchContentType::Image => image_search = Some(gemini::ImageSearch::default()),
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        }
    }
    Some(crate::protocol::wire!(gemini::SearchTypes {
        web_search,
        image_search,
        extra: Default::default(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_responses_web_search_directly_to_gemini_search() {
        let tools = response_tools_to_gemini(Some(vec![openai::ResponseTool::WebSearch {
            filters: None,
            search_context_size: None,
            user_location: None,
            extra: Default::default(),
        }]));
        assert_eq!(tools.len(), 1);
        assert!(tools[0].google_search.is_some());
    }
}
