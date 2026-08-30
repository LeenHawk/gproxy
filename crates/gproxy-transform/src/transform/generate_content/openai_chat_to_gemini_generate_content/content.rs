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
                    contents.push(crate::protocol::wire!(gemini::Content {
                        parts: vec![part],
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
                        extra: Default::default(),
                    }));
                } else {
                    system_parts.push(part);
                }
            }
            openai::ChatCompletionMessageParam::User { content, .. } => {
                seen_non_system = true;
                let parts = chat_content_to_gemini_parts(content);
                if !parts.is_empty() {
                    contents.push(crate::protocol::wire!(gemini::Content {
                        parts,
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
                        extra: Default::default(),
                    }));
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
                        None,
                    ));
                }
                if let Some(tool_calls) = tool_calls {
                    for call in tool_calls {
                        let (id, name, arguments, thought_signature) = match call {
                            openai::ChatToolCall::Function {
                                id,
                                function,
                                mut extra,
                            } => {
                                let thought_signature = take_thought_signature(&mut extra);
                                (id, function.name, function.arguments, thought_signature)
                            }
                            openai::ChatToolCall::Custom {
                                id,
                                custom,
                                mut extra,
                            } => {
                                let thought_signature = take_thought_signature(&mut extra);
                                (id, custom.name, custom.input, thought_signature)
                            }
                            _ => unreachable!(
                                "new non-exhaustive protocol variant requires a lockstep transform update"
                            ),
                        };
                        tool_names.insert(id.clone(), name.clone());
                        parts.push(function_call_part(
                            Some(id),
                            name,
                            arguments,
                            thought_signature,
                        ));
                    }
                }
                if !parts.is_empty() {
                    contents.push(crate::protocol::wire!(gemini::Content {
                        parts,
                        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
                        extra: Default::default(),
                    }));
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
                contents.push(crate::protocol::wire!(gemini::Content {
                    parts: vec![function_response_part(
                        Some(tool_call_id),
                        name,
                        chat_text_content_to_text(content),
                    )],
                    role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
                    extra: Default::default(),
                }));
            }
            openai::ChatCompletionMessageParam::Function { content, name, .. } => {
                seen_non_system = true;
                contents.push(crate::protocol::wire!(gemini::Content {
                    parts: vec![function_response_part(None, name, content)],
                    role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
                    extra: Default::default(),
                }));
            }
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        }
    }

    let contents = merge_adjacent_contents(contents);
    let system_instruction =
        (!system_parts.is_empty()).then_some(crate::protocol::wire!(gemini::Content {
            parts: system_parts,
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)),
            extra: Default::default(),
        }));

    (contents, system_instruction)
}

pub(super) fn text_content_to_gemini_content(
    text: String,
    role: Option<gemini::ContentRole>,
) -> gemini::Content {
    crate::protocol::wire!(gemini::Content {
        parts: vec![text_part(text)],
        role,
        extra: Default::default(),
    })
}

fn chat_text_content_to_text(content: openai::ChatTextContent) -> String {
    match content {
        openai::ChatTextContent::Text(text) => text,
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text { text, .. } => text,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
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
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            })
            .collect(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn chat_content_to_gemini_parts(content: openai::ChatContent) -> Vec<gemini::Part> {
    match content {
        openai::ChatContent::Text(text) => non_empty_text_part(text).into_iter().collect(),
        openai::ChatContent::Parts(parts) => parts
            .into_iter()
            .filter_map(chat_content_part_to_gemini_part)
            .collect(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn chat_content_part_to_gemini_part(part: openai::ChatContentPart) -> Option<gemini::Part> {
    match part {
        openai::ChatContentPart::Text { text, .. } => non_empty_text_part(text),
        openai::ChatContentPart::ImageUrl { image_url, .. } => {
            Some(image_url_to_gemini_part(image_url.url))
        }
        openai::ChatContentPart::File { file, .. } => chat_file_to_gemini_part(file),
        openai::ChatContentPart::InputAudio { input_audio, .. } => Some(crate::protocol::wire!(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: crate::protocol::wire!(gemini::Blob {
                    mime_type: match input_audio.format {
                        openai::InputAudioFormat::Wav => "audio/wav",
                        openai::InputAudioFormat::Mp3 => "audio/mpeg",
                        _ => unreachable!(
                            "new non-exhaustive protocol variant requires a lockstep transform update"
                        ),
                    }
                    .to_owned(),
                    data: input_audio.data,
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        })),
    _ => unreachable!("new non-exhaustive protocol variant requires a lockstep transform update"),
}
}

fn image_url_to_gemini_part(url: String) -> gemini::Part {
    if let Some((mime_type, data)) = parse_data_url(&url) {
        return crate::protocol::wire!(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: crate::protocol::wire!(gemini::Blob {
                    mime_type,
                    data,
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        });
    }

    crate::protocol::wire!(gemini::Part {
        data: Some(crate::protocol::wire!(gemini::PartData::FileData {
            file_data: crate::protocol::wire!(gemini::FileData {
                mime_type: None,
                file_uri: url,
                extra: Default::default(),
            }),
        })),
        ..Default::default()
    })
}

fn chat_file_to_gemini_part(file: openai::ChatFileRef) -> Option<gemini::Part> {
    if let Some(data) = file.file_data {
        return Some(crate::protocol::wire!(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: crate::protocol::wire!(gemini::Blob {
                    mime_type: "application/octet-stream".to_owned(),
                    data,
                    extra: Default::default(),
                }),
            }),
            ..Default::default()
        }));
    }
    file.file_id.map(|file_id| {
        crate::protocol::wire!(gemini::Part {
            data: Some(crate::protocol::wire!(gemini::PartData::FileData {
                file_data: crate::protocol::wire!(gemini::FileData {
                    mime_type: None,
                    file_uri: file_id,
                    extra: Default::default(),
                }),
            })),
            ..Default::default()
        })
    })
}

fn function_call_part(
    id: Option<String>,
    name: String,
    arguments: String,
    thought_signature: Option<String>,
) -> gemini::Part {
    crate::protocol::wire!(gemini::Part {
        thought_signature,
        data: Some(crate::protocol::wire!(gemini::PartData::FunctionCall {
            function_call: crate::protocol::wire!(gemini::FunctionCall {
                id,
                name,
                args: serde_json::from_str(&arguments).ok(),
                extra: Default::default(),
            }),
        })),
        ..Default::default()
    })
}

fn take_thought_signature(extra: &mut openai::Extra) -> Option<String> {
    extra
        .remove("thought_signature")
        .or_else(|| extra.remove("thoughtSignature"))
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
}

fn merge_adjacent_contents(contents: Vec<gemini::Content>) -> Vec<gemini::Content> {
    let mut merged: Vec<gemini::Content> = Vec::new();
    for mut content in contents {
        if content.parts.is_empty() {
            continue;
        }
        if let Some(previous) = merged.last_mut()
            && previous.role == content.role
        {
            previous.parts.append(&mut content.parts);
        } else {
            merged.push(content);
        }
    }
    merged
}

fn function_response_part(id: Option<String>, name: String, output: String) -> gemini::Part {
    let mut response = gemini::JsonMap::new();
    response.insert("output".to_owned(), Value::String(output));
    crate::protocol::wire!(gemini::Part {
        data: Some(crate::protocol::wire!(gemini::PartData::FunctionResponse {
            function_response: crate::protocol::wire!(gemini::FunctionResponse {
                id,
                name,
                response,
                parts: Vec::new(),
                will_continue: None,
                scheduling: None,
                extra: Default::default(),
            }),
        })),
        ..Default::default()
    })
}

fn non_empty_text_part(text: String) -> Option<gemini::Part> {
    (!text.is_empty()).then(|| text_part(text))
}

fn text_part(text: String) -> gemini::Part {
    crate::protocol::wire!(gemini::Part {
        data: Some(gemini::PartData::Text { text }),
        ..Default::default()
    })
}

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let data = url.strip_prefix("data:")?;
    let (mime, payload) = data.split_once(";base64,")?;
    Some((mime.to_owned(), payload.to_owned()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn convert(messages: serde_json::Value) -> serde_json::Value {
        let request: openai::ChatCompletionRequest = serde_json::from_value(json!({
            "model": "gemini-test",
            "messages": messages
        }))
        .unwrap();
        let (contents, _) = chat_messages_to_gemini(request.messages);
        serde_json::to_value(contents).unwrap()
    }

    #[test]
    fn groups_parallel_tool_results_with_following_user_text() {
        let output = convert(json!([
            {"role": "user", "content": "查两个城市"},
            {"role": "assistant", "content": null, "tool_calls": [
                {"id": "call_1", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"北京\"}"}},
                {"id": "call_2", "type": "function", "function": {"name": "weather", "arguments": "{\"city\":\"上海\"}"}}
            ]},
            {"role": "tool", "tool_call_id": "call_1", "content": "晴"},
            {"role": "tool", "tool_call_id": "call_2", "content": "雨"},
            {"role": "user", "content": "总结"}
        ]));

        assert_eq!(output.as_array().unwrap().len(), 3);
        assert_eq!(output[0]["role"], "user");
        assert_eq!(output[1]["role"], "model");
        assert_eq!(output[2]["role"], "user");
        assert_eq!(output[2]["parts"].as_array().unwrap().len(), 3);
        assert_eq!(output[2]["parts"][0]["functionResponse"]["id"], "call_1");
        assert_eq!(output[2]["parts"][1]["functionResponse"]["id"], "call_2");
        assert_eq!(output[2]["parts"][2]["text"], "总结");
    }

    #[test]
    fn invalid_arguments_keep_the_parent_function_call() {
        let output = convert(json!([
            {"role": "user", "content": "查天气"},
            {"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "function": {"name": "weather", "arguments": "{\"city\":\"北京\"}{\"city\":\"上海\"}"}
            }]},
            {"role": "tool", "tool_call_id": "call_1", "content": "参数解析失败"}
        ]));

        assert_eq!(output[1]["parts"][0]["functionCall"]["id"], "call_1");
        assert_eq!(output[1]["parts"][0]["functionCall"]["name"], "weather");
        assert!(output[1]["parts"][0]["functionCall"].get("args").is_none());
        assert_eq!(output[2]["parts"][0]["functionResponse"]["id"], "call_1");
    }

    #[test]
    fn restores_chat_tool_call_thought_signature() {
        let output = convert(json!([
            {"role": "assistant", "content": null, "tool_calls": [{
                "id": "call_1",
                "type": "function",
                "thought_signature": "ciphertext",
                "function": {"name": "weather", "arguments": "{}"}
            }]}
        ]));

        assert_eq!(output[0]["parts"][0]["thoughtSignature"], "ciphertext");
    }
}
