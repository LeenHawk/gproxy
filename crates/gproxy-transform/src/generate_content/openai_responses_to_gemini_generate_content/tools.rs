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
                ..
            } => declarations.push(function(
                name,
                description,
                match parameters {
                    openai::ResponseFunctionParameters::Schema(schema) => Some(schema),
                    openai::ResponseFunctionParameters::Null => None,
                    #[cfg(not(feature = "exhaustive"))]
                    _ => {
                        return Err(crate::TransformError::unsupported(
                            "protocol enum",
                            "unrecognized external variant",
                        ));
                    }
                },
                output_schema,
            )?),
            openai::ResponseTool::Custom {
                name, description, ..
            } => declarations.push(function(name, description, None, None)?),
            openai::ResponseTool::Namespace { tools, .. } => {
                for nested in tools {
                    let (name, description, parameters, output_schema) = match nested {
                        openai::ResponseNamespaceTool::Function {
                            name,
                            description,
                            parameters,
                            output_schema,
                            ..
                        } => (
                            name,
                            description,
                            parameters.and_then(|value| value.as_object().cloned()),
                            output_schema,
                        ),
                        openai::ResponseNamespaceTool::Custom {
                            name, description, ..
                        } => (name, description, None, None),
                        #[cfg(not(feature = "exhaustive"))]
                        _ => {
                            return Err(crate::TransformError::unsupported(
                                "protocol enum",
                                "unrecognized external variant",
                            ));
                        }
                    };
                    declarations.push(function(name, description, parameters, output_schema)?);
                }
            }
            openai::ResponseTool::FileSearch {
                vector_store_ids,
                max_num_results,
                ..
            } => {
                output.push(crate::wire!(gemini::Tool {
                    file_search: Some(gemini::FileSearch {
                        file_search_store_names: vector_store_ids,
                        metadata_filter: None,
                        top_k: max_num_results.map(to_i32).transpose()?,
                        rest: Default::default(),
                    }),
                    ..Default::default()
                }));
            }
            openai::ResponseTool::CollectionsSearch {
                vector_store_ids, ..
            } => output.push(crate::wire!(gemini::Tool {
                file_search: Some(gemini::FileSearch {
                    file_search_store_names: vector_store_ids,
                    metadata_filter: None,
                    top_k: None,
                    rest: Default::default(),
                }),
                ..Default::default()
            })),
            openai::ResponseTool::WebSearch { .. }
            | openai::ResponseTool::WebSearch20250826 { .. } => {
                output.push(crate::wire!(gemini::Tool {
                    google_search: Some(gemini::GoogleSearch::default()),
                    url_context: Some(gemini::UrlContext::default()),
                    rest: Default::default(),
                    ..Default::default()
                }));
            }
            openai::ResponseTool::WebSearchPreview { .. }
            | openai::ResponseTool::WebSearchPreview20250311 { .. }
            | openai::ResponseTool::XSearch { .. } => output.push(crate::wire!(gemini::Tool {
                google_search: Some(gemini::GoogleSearch::default()),
                rest: Default::default(),
                ..Default::default()
            })),
            openai::ResponseTool::CodeExecution { .. }
            | openai::ResponseTool::CodeInterpreter { .. }
            | openai::ResponseTool::Shell { .. }
            | openai::ResponseTool::LocalShell { .. }
            | openai::ResponseTool::ApplyPatch { .. } => output.push(crate::wire!(gemini::Tool {
                code_execution: Some(gemini::CodeExecution::default()),
                rest: Default::default(),
                ..Default::default()
            })),
            openai::ResponseTool::Computer { .. }
            | openai::ResponseTool::ComputerUsePreview { .. } => {
                output.push(crate::wire!(gemini::Tool {
                    computer_use: Some(gemini::ComputerUse {
                        rest: Default::default(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }))
            }
            openai::ResponseTool::Mcp {
                server_label,
                server_url,
                headers,
                ..
            } => output.push(crate::wire!(gemini::Tool {
                mcp_servers: Some(vec![crate::wire!(gemini::McpServer {
                    name: Some(server_label),
                    streamable_http_transport: mcp_transport(server_url, headers),
                    rest: Default::default(),
                })]),
                ..Default::default()
            })),
            openai::ResponseTool::WebFetch { .. }
            | openai::ResponseTool::Memory { .. }
            | openai::ResponseTool::ImageGeneration { .. }
            | openai::ResponseTool::ToolSearch { .. }
            | openai::ResponseTool::ProgrammaticToolCalling { .. } => {}
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
    }
    if !declarations.is_empty() {
        output.insert(
            0,
            crate::wire!(gemini::Tool {
                function_declarations: Some(declarations),
                ..Default::default()
            }),
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
                    #[cfg(not(feature = "exhaustive"))]
                    _ => None,
                })
                .collect();
            (mode, names)
        }
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Unknown(_))
        | openai::ResponseToolChoice::Unknown(_) => return None,
        _ => return None,
    };
    Some(crate::wire!(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(mode)),
            allowed_function_names: (!names.is_empty()).then_some(names),
            rest: Default::default(),
        }),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        rest: Default::default(),
    }))
}

fn function(
    name: String,
    description: Option<String>,
    parameters: Option<openai::JsonSchema>,
    output_schema: Option<openai::JsonSchema>,
) -> Result<gemini::FunctionDeclaration, TransformError> {
    Ok(crate::wire!(gemini::FunctionDeclaration {
        name,
        description: description
            .ok_or_else(|| TransformError::shape("Responses tool", "description is missing"))?,
        behavior: None,
        parameters: None,
        parameters_json_schema: parameters.map(serde_json::Value::Object),
        response: None,
        response_json_schema: output_schema.map(serde_json::Value::Object),
        rest: Default::default(),
    }))
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
    Some(crate::wire!(gemini::StreamableHttpTransport {
        url,
        headers,
        timeout: None,
        sse_read_timeout: None,
        terminate_on_close: None,
        rest: Default::default(),
    }))
}
