use serde_json::Value;

use crate::protocol::{gemini, openai};

pub(super) fn gemini_tools_to_responses(
    tools: Vec<gemini::Tool>,
) -> Option<Vec<openai::ResponseTool>> {
    let mut output = Vec::new();
    let mut has_web_search = false;
    for tool in tools {
        output.extend(tool.function_declarations.into_iter().map(|function| {
            openai::ResponseTool::Function {
                name: function.name,
                parameters: function
                    .parameters_json_schema
                    .or_else(|| function.parameters.map(json_value))
                    .and_then(json_object)
                    .unwrap_or_default(),
                strict: Some(false),
                defer_loading: None,
                description: (!function.description.is_empty()).then_some(function.description),
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
            if !has_web_search {
                output.push(openai::ResponseTool::WebSearchPreview {
                    search_content_types: tool
                        .google_search
                        .as_ref()
                        .and_then(gemini_search_content_types),
                    search_context_size: None,
                    user_location: None,
                    extra: Default::default(),
                });
                has_web_search = true;
            }
        }
        if (tool.url_context.is_some() || tool.google_maps.is_some()) && !has_web_search {
            output.push(openai::ResponseTool::WebSearch {
                filters: None,
                search_context_size: None,
                user_location: None,
                extra: Default::default(),
            });
            has_web_search = true;
        }
        if tool.code_execution.is_some() {
            output.push(openai::ResponseTool::CodeInterpreter {
                container: openai::CodeInterpreterContainer::Auto(crate::protocol::wire!(
                    openai::CodeInterpreterAutoContainer {
                        type_: openai::CodeInterpreterContainerType::Auto,
                        file_ids: None,
                        memory_limit: None,
                        network_policy: None,
                        extra: Default::default(),
                    }
                )),
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
    (!output.is_empty()).then_some(output)
}

pub(super) fn gemini_tool_config_to_responses(
    config: Option<gemini::ToolConfig>,
) -> Option<openai::ResponseToolChoice> {
    let config = config?.function_calling_config?;
    let names = config.allowed_function_names;
    match config.mode? {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => Some(
            openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None),
        ),
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto) => {
            if names.is_empty() {
                Some(openai::ResponseToolChoice::Mode(
                    openai::ToolChoiceMode::Auto,
                ))
            } else {
                Some(allowed_choice(openai::AllowedToolsMode::Auto, names))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            if names.len() == 1 {
                Some(openai::ResponseToolChoice::Function(
                    crate::protocol::wire!(openai::ResponseFunctionToolChoice {
                        type_: openai::FunctionToolChoiceType::Function,
                        name: names.into_iter().next().unwrap_or_default(),
                        extra: Default::default(),
                    }),
                ))
            } else if names.is_empty() {
                Some(openai::ResponseToolChoice::Mode(
                    openai::ToolChoiceMode::Required,
                ))
            } else {
                Some(allowed_choice(openai::AllowedToolsMode::Required, names))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified)
        | gemini::FunctionCallingMode::Unknown(_) => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn allowed_choice(
    mode: openai::AllowedToolsMode,
    names: Vec<String>,
) -> openai::ResponseToolChoice {
    openai::ResponseToolChoice::Allowed(crate::protocol::wire!(openai::ResponseAllowedToolChoice {
        mode,
        tools: names
            .into_iter()
            .map(|name| openai::ResponseAllowedTool::Function {
                name,
                extra: Default::default(),
            })
            .collect(),
        type_: openai::AllowedToolsType::AllowedTools,
        extra: Default::default(),
    }))
}

fn json_value<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn json_object(value: Value) -> Option<openai::JsonSchema> {
    match value {
        Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn gemini_search_content_types(
    search: &gemini::GoogleSearch,
) -> Option<Vec<openai::SearchContentType>> {
    let search_types = search.search_types.as_ref()?;
    let mut output = Vec::new();
    if search_types.web_search.is_some() {
        output.push(openai::SearchContentType::Text);
    }
    if search_types.image_search.is_some() {
        output.push(openai::SearchContentType::Image);
    }
    (!output.is_empty()).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_all_allowed_function_names_without_chat_narrowing() {
        let config: gemini::ToolConfig = serde_json::from_value(serde_json::json!({
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": ["first", "second"]
            }
        }))
        .unwrap();
        let choice = gemini_tool_config_to_responses(Some(config)).unwrap();
        let openai::ResponseToolChoice::Allowed(choice) = choice else {
            panic!("expected allowed-tools choice");
        };
        assert_eq!(choice.tools.len(), 2);
    }
}
