use std::collections::BTreeMap;

use serde_json::Value;

use crate::protocol::{gemini, openai};

pub(super) fn chat_messages_to_gemini(
    messages: Vec<openai::ChatCompletionMessageParam>,
) -> (Vec<gemini::Content>, Option<gemini::Content>) {
    let mut contents = Vec::new();
    let mut system_parts = Vec::new();
    let mut seen_non_system = false;
    let mut tool_names = BTreeMap::new();

    for message in messages {
        match message {
            openai::ChatCompletionMessageParam::Developer { content, .. }
            | openai::ChatCompletionMessageParam::System { content, .. } => {
                let text = chat_text_content_to_text(content);
                if text.is_empty() {
                    continue;
                }
                let part = text_part(text);
                if seen_non_system {
                    contents.push(gemini::Content {
                        parts: vec![part],
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
                        extra: Default::default(),
                    });
                } else {
                    system_parts.push(part);
                }
            }
            openai::ChatCompletionMessageParam::User { content, .. } => {
                seen_non_system = true;
                let parts = chat_content_to_gemini_parts(content);
                if !parts.is_empty() {
                    contents.push(gemini::Content {
                        parts,
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
                        extra: Default::default(),
                    });
                }
            }
            openai::ChatCompletionMessageParam::Assistant {
                content,
                function_call,
                refusal,
                tool_calls,
                ..
            } => {
                seen_non_system = true;
                let mut parts = Vec::new();
                if let Some(content) = content {
                    parts.extend(chat_assistant_content_to_gemini_parts(content));
                }
                if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
                    parts.push(text_part(refusal));
                }
                if let Some(function_call) = function_call {
                    tool_names.insert("function_call".to_owned(), function_call.name.clone());
                    parts.push(function_call_part(
                        Some("function_call".to_owned()),
                        function_call.name,
                        function_call.arguments,
                    ));
                }
                if let Some(tool_calls) = tool_calls {
                    for call in tool_calls {
                        let (id, name, arguments) = match call {
                            openai::ChatToolCall::Function { id, function, .. } => {
                                (id, function.name, function.arguments)
                            }
                            openai::ChatToolCall::Custom { id, custom, .. } => {
                                (id, custom.name, custom.input)
                            }
                        };
                        tool_names.insert(id.clone(), name.clone());
                        parts.push(function_call_part(Some(id), name, arguments));
                    }
                }
                if !parts.is_empty() {
                    contents.push(gemini::Content {
                        parts,
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
                        extra: Default::default(),
                    });
                }
            }
            openai::ChatCompletionMessageParam::Tool {
                content,
                tool_call_id,
                ..
            } => {
                seen_non_system = true;
                let name = tool_names
                    .get(&tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| tool_call_id.clone());
                contents.push(gemini::Content {
                    parts: vec![function_response_part(
                        Some(tool_call_id),
                        name,
                        chat_text_content_to_text(content),
                    )],
                    role: Some(gemini::ContentRole::Known(
                        gemini::ContentRoleKnown::Function,
                    )),
                    extra: Default::default(),
                });
            }
            openai::ChatCompletionMessageParam::Function { content, name, .. } => {
                seen_non_system = true;
                contents.push(gemini::Content {
                    parts: vec![function_response_part(None, name, content)],
                    role: Some(gemini::ContentRole::Known(
                        gemini::ContentRoleKnown::Function,
                    )),
                    extra: Default::default(),
                });
            }
        }
    }

    let system_instruction = (!system_parts.is_empty()).then_some(gemini::Content {
        parts: system_parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
        extra: Default::default(),
    });

    (contents, system_instruction)
}

pub(super) fn text_content_to_gemini_content(
    text: String,
    role: Option<gemini::ContentRole>,
) -> gemini::Content {
    gemini::Content {
        parts: vec![text_part(text)],
        role,
        extra: Default::default(),
    }
}

fn chat_text_content_to_text(content: openai::ChatTextContent) -> String {
    match content {
        openai::ChatTextContent::Text(text) => text,
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text { text, .. } => text,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

fn chat_assistant_content_to_gemini_parts(
    content: openai::ChatAssistantContent,
) -> Vec<gemini::Part> {
    match content {
        openai::ChatAssistantContent::Text(text) => non_empty_text_part(text).into_iter().collect(),
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| match part {
                openai::ChatAssistantContentPart::Text { text, .. } => non_empty_text_part(text),
                openai::ChatAssistantContentPart::Refusal { refusal, .. } => {
                    non_empty_text_part(refusal)
                }
            })
            .collect(),
    }
}

fn chat_content_to_gemini_parts(content: openai::ChatContent) -> Vec<gemini::Part> {
    match content {
        openai::ChatContent::Text(text) => non_empty_text_part(text).into_iter().collect(),
        openai::ChatContent::Parts(parts) => parts
            .into_iter()
            .filter_map(chat_content_part_to_gemini_part)
            .collect(),
    }
}

fn chat_content_part_to_gemini_part(part: openai::ChatContentPart) -> Option<gemini::Part> {
    match part {
        openai::ChatContentPart::Text { text, .. } => non_empty_text_part(text),
        openai::ChatContentPart::ImageUrl { image_url, .. } => {
            Some(image_url_to_gemini_part(image_url.url))
        }
        openai::ChatContentPart::File { file, .. } => chat_file_to_gemini_part(file),
        openai::ChatContentPart::InputAudio { input_audio, .. } => Some(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: gemini::Blob {
                    mime_type: match input_audio.format {
                        openai::InputAudioFormat::Wav => "audio/wav",
                        openai::InputAudioFormat::Mp3 => "audio/mpeg",
                    }
                    .to_owned(),
                    data: input_audio.data,
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        }),
    }
}

fn image_url_to_gemini_part(url: String) -> gemini::Part {
    if let Some((mime_type, data)) = parse_data_url(&url) {
        return gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: gemini::Blob {
                    mime_type,
                    data,
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        };
    }

    gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type: None,
                file_uri: url,
                extra: Default::default(),
            },
        }),
        ..Default::default()
    }
}

fn chat_file_to_gemini_part(file: openai::ChatFileRef) -> Option<gemini::Part> {
    if let Some(data) = file.file_data {
        return Some(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: gemini::Blob {
                    mime_type: "application/octet-stream".to_owned(),
                    data,
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        });
    }
    file.file_id.map(|file_id| gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type: None,
                file_uri: file_id,
                extra: Default::default(),
            },
        }),
        ..Default::default()
    })
}

fn function_call_part(id: Option<String>, name: String, arguments: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id,
                name,
                args: serde_json::from_str(&arguments).ok(),
                extra: Default::default(),
            },
        }),
        ..Default::default()
    }
}

fn function_response_part(id: Option<String>, name: String, output: String) -> gemini::Part {
    let mut response = gemini::JsonMap::new();
    response.insert("output".to_owned(), Value::String(output));
    gemini::Part {
        data: Some(gemini::PartData::FunctionResponse {
            function_response: gemini::FunctionResponse {
                id,
                name,
                response,
                parts: Vec::new(),
                will_continue: None,
                scheduling: None,
                extra: Default::default(),
            },
        }),
        ..Default::default()
    }
}

fn non_empty_text_part(text: String) -> Option<gemini::Part> {
    (!text.is_empty()).then(|| text_part(text))
}

fn text_part(text: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::Text { text }),
        ..Default::default()
    }
}

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let data = url.strip_prefix("data:")?;
    let (mime, payload) = data.split_once(";base64,")?;
    Some((mime.to_owned(), payload.to_owned()))
}
