use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn to_gemini(
    tools: Option<Vec<openai::ResponseTool>>,
) -> Result<Option<Vec<gemini::Tool>>, TransformError> {
    let mut declarations = Vec::new();
    let mut output = Vec::new();
    for tool in tools.into_iter().flatten() {
        match tool.type_.clone() {
            openai::ToolType::Function | openai::ToolType::Custom => declarations.push(function(
                tool.name,
                tool.description,
                tool.parameters,
                tool.rest,
            )?),
            openai::ToolType::Namespace => {
                for nested in tool.tools.into_iter().flatten() {
                    if matches!(
                        nested.type_,
                        openai::ToolType::Function | openai::ToolType::Custom
                    ) {
                        declarations.push(function(
                            Some(nested.name),
                            nested.description,
                            nested
                                .parameters
                                .and_then(|value| value.as_object().cloned()),
                            nested.rest,
                        )?);
                    }
                }
            }
            openai::ToolType::FileSearch | openai::ToolType::CollectionsSearch => {
                let stores = tool.vector_store_ids.ok_or_else(|| {
                    TransformError::shape("Responses file search tool", "vector_store_ids missing")
                })?;
                output.push(gemini::Tool {
                    file_search: Some(gemini::FileSearch {
                        file_search_store_names: stores,
                        metadata_filter: None,
                        top_k: tool.max_num_results.map(to_i32).transpose()?,
                        rest: tool.rest,
                    }),
                    ..Default::default()
                });
            }
            openai::ToolType::WebSearch
            | openai::ToolType::WebSearch20250826
            | openai::ToolType::WebSearchPreview
            | openai::ToolType::WebSearchPreview20250311
            | openai::ToolType::XSearch => output.push(gemini::Tool {
                google_search: Some(gemini::GoogleSearch::default()),
                url_context: matches!(
                    tool.type_,
                    openai::ToolType::WebSearch | openai::ToolType::WebSearch20250826
                )
                .then(gemini::UrlContext::default),
                rest: tool.rest,
                ..Default::default()
            }),
            openai::ToolType::CodeExecution
            | openai::ToolType::CodeInterpreter
            | openai::ToolType::Shell
            | openai::ToolType::LocalShell
            | openai::ToolType::ApplyPatch => output.push(gemini::Tool {
                code_execution: Some(gemini::CodeExecution::default()),
                rest: tool.rest,
                ..Default::default()
            }),
            openai::ToolType::Computer
            | openai::ToolType::ComputerUse
            | openai::ToolType::ComputerUsePreview => output.push(gemini::Tool {
                computer_use: Some(gemini::ComputerUse {
                    rest: tool.rest,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            openai::ToolType::Mcp => output.push(gemini::Tool {
                mcp_servers: Some(vec![gemini::McpServer {
                    name: tool.server_label,
                    streamable_http_transport: mcp_transport(tool.server_url, tool.headers),
                    rest: tool.rest,
                }]),
                ..Default::default()
            }),
            openai::ToolType::Unknown(value) => {
                return Err(TransformError::unsupported("Responses tool", value));
            }
            other => {
                return Err(TransformError::unsupported(
                    "Responses tool",
                    other.as_str(),
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
                .filter_map(|tool| tool.name)
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
    name: Option<String>,
    description: Option<String>,
    parameters: Option<openai::JsonSchema>,
    rest: openai::Rest,
) -> Result<gemini::FunctionDeclaration, TransformError> {
    Ok(gemini::FunctionDeclaration {
        name: name.ok_or_else(|| TransformError::shape("Responses tool", "name is missing"))?,
        description: description
            .ok_or_else(|| TransformError::shape("Responses tool", "description is missing"))?,
        behavior: None,
        parameters: None,
        parameters_json_schema: parameters.map(serde_json::Value::Object),
        response: None,
        response_json_schema: None,
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
