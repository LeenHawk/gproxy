use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn to_gemini(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Result<Option<Vec<gemini::Tool>>, TransformError> {
    let mut declarations = Vec::new();
    let mut output = Vec::new();
    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ResponseTool::Function {
                name,
                description,
                parameters,
                output_schema,
                rest,
                ..
            } => declarations.push(function(
                name,
                description,
                match parameters {
                    openai::ResponseFunctionParameters::Schema(schema) => Some(schema),
                    openai::ResponseFunctionParameters::Null => None,
                },
                output_schema,
                rest,
            )?),
            openai::ResponseTool::Custom {
                name,
                description,
                rest,
                ..
            } => declarations.push(function(name, description, None, None, rest)?),
            openai::ResponseTool::Namespace { tools, .. } => {
                for nested in tools {
                    let (name, description, parameters, output_schema, rest) = match nested {
                        openai::ResponseNamespaceTool::Function {
                            name,
                            description,
                            parameters,
                            output_schema,
                            rest,
                            ..
                        } => (
                            name,
                            description,
                            parameters.and_then(|value| value.as_object().cloned()),
                            output_schema,
                            rest,
                        ),
                        openai::ResponseNamespaceTool::Custom {
                            name,
                            description,
                            rest,
                            ..
                        } => (name, description, None, None, rest),
                    };
                    declarations.push(function(
                        name,
                        description,
                        parameters,
                        output_schema,
                        rest,
                    )?);
                }
            }
            openai::ResponseTool::FileSearch {
                vector_store_ids,
                max_num_results,
                rest,
                ..
            } => {
                output.push(gemini::Tool {
                    file_search: Some(gemini::FileSearch {
                        file_search_store_names: vector_store_ids,
                        metadata_filter: None,
                        top_k: max_num_results.map(to_i32).transpose()?,
                        rest,
                    }),
                    ..Default::default()
                });
            }
            openai::ResponseTool::CollectionsSearch {
                vector_store_ids,
                rest,
            } => output.push(gemini::Tool {
                file_search: Some(gemini::FileSearch {
                    file_search_store_names: vector_store_ids,
                    metadata_filter: None,
                    top_k: None,
                    rest,
                }),
                ..Default::default()
            }),
            openai::ResponseTool::WebSearch { rest, .. }
            | openai::ResponseTool::WebSearch20250826 { rest, .. } => {
                output.push(gemini::Tool {
                    google_search: Some(gemini::GoogleSearch::default()),
                    url_context: Some(gemini::UrlContext::default()),
                    rest,
                    ..Default::default()
                });
            }
            openai::ResponseTool::WebSearchPreview { rest, .. }
            | openai::ResponseTool::WebSearchPreview20250311 { rest, .. }
            | openai::ResponseTool::XSearch { rest } => output.push(gemini::Tool {
                google_search: Some(gemini::GoogleSearch::default()),
                rest,
                ..Default::default()
            }),
            openai::ResponseTool::CodeExecution { rest }
            | openai::ResponseTool::CodeInterpreter { rest, .. }
            | openai::ResponseTool::Shell { rest, .. }
            | openai::ResponseTool::LocalShell { rest }
            | openai::ResponseTool::ApplyPatch { rest, .. } => output.push(gemini::Tool {
                code_execution: Some(gemini::CodeExecution::default()),
                rest,
                ..Default::default()
            }),
            openai::ResponseTool::Computer { rest }
            | openai::ResponseTool::ComputerUsePreview { rest, .. } => output.push(gemini::Tool {
                computer_use: Some(gemini::ComputerUse {
                    rest,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            openai::ResponseTool::Mcp {
                server_label,
                server_url,
                headers,
                rest,
                ..
            } => output.push(gemini::Tool {
                mcp_servers: Some(vec![gemini::McpServer {
                    name: Some(server_label),
                    streamable_http_transport: mcp_transport(server_url, headers),
                    rest,
                }]),
                ..Default::default()
            }),
            openai::ResponseTool::WebFetch { .. } => {
                return Err(TransformError::unsupported("Responses tool", "web_fetch"));
            }
            openai::ResponseTool::Memory { .. } => {
                return Err(TransformError::unsupported("Responses tool", "memory"));
            }
            openai::ResponseTool::ImageGeneration { .. } => {
                return Err(TransformError::unsupported(
                    "Responses tool",
                    "image_generation",
                ));
            }
            openai::ResponseTool::ToolSearch { .. } => {
                return Err(TransformError::unsupported("Responses tool", "tool_search"));
            }
            openai::ResponseTool::ProgrammaticToolCalling { .. } => {
                return Err(TransformError::unsupported(
                    "Responses tool",
                    "programmatic_tool_calling",
                ));
            }
        }
    }
    if !declarations.is_empty() {
        output.insert(
            0,
            gemini::Tool {
                function_declarations: Some(declarations),
                ..Default::default()
            },
        );
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(super) fn choice_to_gemini(
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
        openai::ResponseToolChoice::Function(value) => {
            (gemini::FunctionCallingModeKnown::Any, vec![value.name])
        }
        openai::ResponseToolChoice::Custom(value) => {
            (gemini::FunctionCallingModeKnown::Any, vec![value.name])
        }
        openai::ResponseToolChoice::Allowed(value) => {
            let mode = match value.mode {
                openai::AllowedToolsMode::Auto => gemini::FunctionCallingModeKnown::Auto,
                _ => gemini::FunctionCallingModeKnown::Any,
            };
            let names = value
                .tools
                .into_iter()
                .filter_map(|tool| match tool {
                    openai::ResponseAllowedTool::Function { name, .. }
                    | openai::ResponseAllowedTool::Custom { name, .. } => Some(name),
                    openai::ResponseAllowedTool::Mcp { name, .. } => name,
                    openai::ResponseAllowedTool::FileSearch { .. }
                    | openai::ResponseAllowedTool::WebSearchPreview { .. }
                    | openai::ResponseAllowedTool::Computer { .. }
                    | openai::ResponseAllowedTool::ComputerUsePreview { .. }
                    | openai::ResponseAllowedTool::ComputerUse { .. }
                    | openai::ResponseAllowedTool::WebSearchPreview20250311 { .. }
                    | openai::ResponseAllowedTool::ImageGeneration { .. }
                    | openai::ResponseAllowedTool::CodeInterpreter { .. }
                    | openai::ResponseAllowedTool::LocalShell { .. }
                    | openai::ResponseAllowedTool::Shell { .. }
                    | openai::ResponseAllowedTool::ApplyPatch { .. } => None,
                })
                .collect();
            (mode, names)
        }
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Unknown(_))
        | openai::ResponseToolChoice::Unknown(_) => return None,
        _ => return None,
    };
    Some(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(mode)),
            allowed_function_names: (!names.is_empty()).then_some(names),
            rest: Default::default(),
        }),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        rest: Default::default(),
    })
}

fn function(
    name: String,
    description: Option<String>,
    parameters: Option<openai::JsonSchema>,
    output_schema: Option<openai::JsonSchema>,
    rest: openai::Rest,
) -> Result<gemini::FunctionDeclaration, TransformError> {
    Ok(gemini::FunctionDeclaration {
        name,
        description: description
            .ok_or_else(|| TransformError::shape("Responses tool", "description is missing"))?,
        behavior: None,
        parameters: None,
        parameters_json_schema: parameters.map(serde_json::Value::Object),
        response: None,
        response_json_schema: output_schema.map(serde_json::Value::Object),
        rest,
    })
}

fn to_i32(value: u32) -> Result<i32, TransformError> {
    i32::try_from(value).map_err(|_| {
        TransformError::shape("Responses file search tool", "max_num_results exceeds i32")
    })
}

fn mcp_transport(
    url: Option<String>,
    headers: Option<std::collections::BTreeMap<String, String>>,
) -> Option<gemini::StreamableHttpTransport> {
    if url.is_none() && headers.is_none() {
        return None;
    }
    Some(gemini::StreamableHttpTransport {
        url,
        headers,
        timeout: None,
        sse_read_timeout: None,
        terminate_on_close: None,
        rest: Default::default(),
    })
}
