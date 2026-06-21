use crate::protocol::openai;

#[derive(Default)]
pub(super) struct ResponseToolsForChat {
    pub tools: Option<Vec<openai::ChatTool>>,
    pub web_search_options: Option<openai::ChatWebSearchOptions>,
}

pub(super) fn response_tools_for_chat(
    tools: Option<Vec<openai::ResponseTool>>,
) -> ResponseToolsForChat {
    let Some(tools) = tools else {
        return ResponseToolsForChat::default();
    };

    let mut chat_tools = Vec::new();
    let mut web_search_options = None;

    for tool in tools {
        match tool {
            openai::ResponseTool::WebSearch {
                search_context_size,
                user_location,
                ..
            }
            | openai::ResponseTool::WebSearch20250826 {
                search_context_size,
                user_location,
                ..
            } => {
                web_search_options = Some(openai::ChatWebSearchOptions {
                    search_context_size,
                    user_location: user_location.map(web_search_location_to_chat),
                    extra: Default::default(),
                });
            }
            openai::ResponseTool::WebSearchPreview {
                search_context_size,
                user_location,
                ..
            }
            | openai::ResponseTool::WebSearchPreview20250311 {
                search_context_size,
                user_location,
                ..
            } => {
                web_search_options = Some(openai::ChatWebSearchOptions {
                    search_context_size,
                    user_location: user_location.map(web_search_preview_location_to_chat),
                    extra: Default::default(),
                });
            }
            tool => {
                if let Some(tool) = response_tool_to_chat_tool(tool) {
                    chat_tools.push(tool);
                }
            }
        }
    }

    ResponseToolsForChat {
        tools: (!chat_tools.is_empty()).then_some(chat_tools),
        web_search_options,
    }
}

pub(super) fn response_tool_choice_to_chat_tool_choice(
    choice: Option<openai::ResponseToolChoice>,
) -> Option<openai::ChatToolChoice> {
    Some(match choice? {
        openai::ResponseToolChoice::Mode(mode) => openai::ChatToolChoice::Mode(mode),
        openai::ResponseToolChoice::Function(choice) => {
            openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Function {
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::NamedTool {
                    name: choice.name,
                    extra: Default::default(),
                },
                extra: Default::default(),
            })
        }
        openai::ResponseToolChoice::Custom(choice) => {
            openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Custom {
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::NamedTool {
                    name: choice.name,
                    extra: Default::default(),
                },
                extra: Default::default(),
            })
        }
        _ => return None,
    })
}

pub(super) fn function_call_to_chat_tool_call(
    call_id: String,
    name: String,
    arguments: String,
) -> openai::ChatToolCall {
    openai::ChatToolCall::Function {
        id: call_id,
        function: openai::FunctionCall {
            arguments,
            name,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

pub(super) fn custom_call_to_chat_tool_call(
    call_id: String,
    name: String,
    input: String,
) -> openai::ChatToolCall {
    openai::ChatToolCall::Custom {
        id: call_id,
        custom: openai::CustomToolCall {
            input,
            name,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

fn response_tool_to_chat_tool(tool: openai::ResponseTool) -> Option<openai::ChatTool> {
    match tool {
        openai::ResponseTool::Function {
            name,
            parameters,
            strict,
            description,
            ..
        } => Some(openai::ChatTool::Function {
            function: openai::FunctionDefinition {
                name,
                description,
                parameters: Some(parameters),
                strict: Some(strict),
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
        openai::ResponseTool::Custom {
            name,
            description,
            format,
            ..
        } => Some(openai::ChatTool::Custom {
            custom: openai::CustomToolDefinition {
                name,
                description,
                format,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
        _ => None,
    }
}

fn web_search_location_to_chat(
    location: openai::WebSearchUserLocation,
) -> openai::ChatWebSearchUserLocation {
    openai::ChatWebSearchUserLocation {
        approximate: openai::ApproximateLocation {
            city: location.city,
            country: location.country,
            region: location.region,
            timezone: location.timezone,
            extra: Default::default(),
        },
        type_: location
            .type_
            .unwrap_or(openai::ApproximateLocationType::Approximate),
        extra: Default::default(),
    }
}

fn web_search_preview_location_to_chat(
    location: openai::WebSearchPreviewUserLocation,
) -> openai::ChatWebSearchUserLocation {
    openai::ChatWebSearchUserLocation {
        approximate: openai::ApproximateLocation {
            city: location.city,
            country: location.country,
            region: location.region,
            timezone: location.timezone,
            extra: Default::default(),
        },
        type_: location.type_,
        extra: Default::default(),
    }
}
