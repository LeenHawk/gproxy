use crate::protocol::{gemini, openai};

pub(super) fn gemini_contents_to_chat_messages(
    contents: Vec<gemini::Content>,
) -> Vec<openai::ChatCompletionMessageParam> {
    let mut messages = Vec::new();
    for content in contents {
        let role = content.role.clone();
        match role {
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => {
                messages.push(gemini_content_to_assistant_param(content));
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
                let text = gemini_content_to_text(content);
                if !text.is_empty() {
                    messages.push(openai::ChatCompletionMessageParam::Developer {
                        content: openai::ChatTextContent::Text(text),
                        name: None,
                        extra: Default::default(),
                    });
                }
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Function)) => {
                messages.extend(gemini_content_to_tool_messages(content));
            }
            Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User))
            | Some(gemini::ContentRole::Unknown(_))
            | None => {
                if let Some(message) = gemini_content_to_user_message(content) {
                    messages.push(message);
                }
            }
        }
    }
    messages
}

pub(super) fn gemini_content_to_text(content: gemini::Content) -> String {
    content
        .parts
        .into_iter()
        .filter_map(|part| match part.data {
            Some(gemini::PartData::Text { text }) => Some(text),
            Some(gemini::PartData::ExecutableCode { executable_code }) => {
                Some(executable_code.code)
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
            }) => code_execution_result.output,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn gemini_content_to_chat_message(content: gemini::Content) -> openai::ChatMessage {
    let openai::ChatCompletionMessageParam::Assistant {
        content,
        tool_calls,
        ..
    } = gemini_content_to_assistant_param(content)
    else {
        return openai::ChatMessage {
            role: openai::ChatCompletionMessageRole::Assistant,
            content: Some(String::new()),
            refusal: None,
            annotations: None,
            audio: None,
            function_call: None,
            reasoning_content: None,
            tool_calls: None,
            extra: Default::default(),
        };
    };

    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: content.map(chat_assistant_content_to_text),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: None,
        tool_calls,
        extra: Default::default(),
    }
}

fn gemini_content_to_assistant_param(
    content: gemini::Content,
) -> openai::ChatCompletionMessageParam {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for part in content.parts {
        match part.data {
            Some(gemini::PartData::Text { text }) => text_parts.push(text),
            Some(gemini::PartData::FunctionCall { function_call }) => {
                tool_calls.push(openai::ChatToolCall::Function {
                    id: function_call
                        .id
                        .unwrap_or_else(|| format!("call_{}", function_call.name)),
                    function: openai::FunctionCall {
                        arguments: function_call
                            .args
                            .map(|args| {
                                serde_json::to_string(&args).unwrap_or_else(|_| "{}".to_owned())
                            })
                            .unwrap_or_else(|| "{}".to_owned()),
                        name: function_call.name,
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            Some(gemini::PartData::ExecutableCode { executable_code }) => {
                text_parts.push(executable_code.code);
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
            }) => {
                if let Some(output) = code_execution_result.output {
                    text_parts.push(output);
                }
            }
            _ => {}
        }
    }

    openai::ChatCompletionMessageParam::Assistant {
        content: (!text_parts.is_empty())
            .then(|| openai::ChatAssistantContent::Text(text_parts.join("\n"))),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        extra: Default::default(),
    }
}

fn gemini_content_to_user_message(
    content: gemini::Content,
) -> Option<openai::ChatCompletionMessageParam> {
    let parts = content
        .parts
        .into_iter()
        .filter_map(gemini_part_to_chat_content_part)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let content = if parts.len() == 1 {
        match parts.into_iter().next() {
            Some(openai::ChatContentPart::Text { text, .. }) => openai::ChatContent::Text(text),
            Some(part) => openai::ChatContent::Parts(vec![part]),
            None => return None,
        }
    } else {
        openai::ChatContent::Parts(parts)
    };
    Some(openai::ChatCompletionMessageParam::User {
        content,
        name: None,
        extra: Default::default(),
    })
}

fn gemini_content_to_tool_messages(
    content: gemini::Content,
) -> Vec<openai::ChatCompletionMessageParam> {
    content
        .parts
        .into_iter()
        .filter_map(|part| match part.data {
            Some(gemini::PartData::FunctionResponse { function_response }) => {
                Some(openai::ChatCompletionMessageParam::Tool {
                    content: openai::ChatTextContent::Text(function_response_to_text(
                        &function_response,
                    )),
                    tool_call_id: function_response.id.unwrap_or(function_response.name),
                    extra: Default::default(),
                })
            }
            Some(gemini::PartData::ToolResponse { tool_response }) => {
                Some(openai::ChatCompletionMessageParam::Tool {
                    content: openai::ChatTextContent::Text(
                        tool_response
                            .response
                            .map(|response| {
                                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_owned())
                            })
                            .unwrap_or_default(),
                    ),
                    tool_call_id: tool_response
                        .id
                        .unwrap_or_else(|| "tool_response".to_owned()),
                    extra: Default::default(),
                })
            }
            _ => None,
        })
        .collect()
}

fn gemini_part_to_chat_content_part(part: gemini::Part) -> Option<openai::ChatContentPart> {
    match part.data? {
        gemini::PartData::Text { text } => Some(openai::ChatContentPart::Text {
            text,
            extra: Default::default(),
        }),
        gemini::PartData::InlineData { inline_data } => {
            inline_data_to_chat_part(inline_data.mime_type, inline_data.data)
        }
        gemini::PartData::FileData { file_data } => file_data_to_chat_part(file_data),
        gemini::PartData::ExecutableCode { executable_code } => {
            Some(openai::ChatContentPart::Text {
                text: executable_code.code,
                extra: Default::default(),
            })
        }
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
        } => code_execution_result
            .output
            .map(|text| openai::ChatContentPart::Text {
                text,
                extra: Default::default(),
            }),
        _ => None,
    }
}

fn inline_data_to_chat_part(mime_type: String, data: String) -> Option<openai::ChatContentPart> {
    if mime_type.starts_with("image/") {
        return Some(openai::ChatContentPart::ImageUrl {
            image_url: openai::ImageUrl {
                url: format!("data:{mime_type};base64,{data}"),
                detail: None,
                extra: Default::default(),
            },
            extra: Default::default(),
        });
    }
    match mime_type.as_str() {
        "audio/wav" => Some(openai::ChatContentPart::InputAudio {
            input_audio: openai::InputAudio {
                data,
                format: openai::InputAudioFormat::Wav,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
        "audio/mpeg" | "audio/mp3" => Some(openai::ChatContentPart::InputAudio {
            input_audio: openai::InputAudio {
                data,
                format: openai::InputAudioFormat::Mp3,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
        _ => Some(openai::ChatContentPart::File {
            file: openai::ChatFileRef {
                file_data: Some(data),
                file_id: None,
                filename: None,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
    }
}

fn file_data_to_chat_part(file_data: gemini::FileData) -> Option<openai::ChatContentPart> {
    if file_data
        .mime_type
        .as_ref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return Some(openai::ChatContentPart::ImageUrl {
            image_url: openai::ImageUrl {
                url: file_data.file_uri,
                detail: None,
                extra: Default::default(),
            },
            extra: Default::default(),
        });
    }
    Some(openai::ChatContentPart::File {
        file: openai::ChatFileRef {
            file_data: None,
            file_id: Some(file_data.file_uri),
            filename: None,
            extra: Default::default(),
        },
        extra: Default::default(),
    })
}

fn function_response_to_text(response: &gemini::FunctionResponse) -> String {
    response
        .response
        .get("output")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string(&response.response).unwrap_or_default())
}

fn chat_assistant_content_to_text(content: openai::ChatAssistantContent) -> String {
    match content {
        openai::ChatAssistantContent::Text(text) => text,
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatAssistantContentPart::Text { text, .. } => text,
                openai::ChatAssistantContentPart::Refusal { refusal, .. } => refusal,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}
