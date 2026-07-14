use crate::protocol::openai;

use super::util::{response_detail_to_chat_detail, response_output_to_chat_content};
use crate::transform::generate_content::openai_responses_to_openai_chat::tools::{
    custom_call_to_chat_tool_call, function_call_to_chat_tool_call,
};

pub(in crate::transform::generate_content::openai_responses_to_openai_chat) fn response_input_to_chat_messages(
    input: Option<openai::ResponseInput>,
) -> Vec<openai::ChatCompletionMessageParam> {
    match input {
        Some(openai::ResponseInput::Text(text)) => {
            vec![openai::ChatCompletionMessageParam::User {
                content: openai::ChatContent::Text(text),
                name: None,
                extra: Default::default(),
            }]
        }
        Some(openai::ResponseInput::Items(items)) => response_items_to_chat_messages(items),
        None => Vec::new(),
    }
}

fn response_items_to_chat_messages(
    items: Vec<openai::ResponseItem>,
) -> Vec<openai::ChatCompletionMessageParam> {
    let mut messages = Vec::new();
    let mut pending_reasoning = Vec::new();

    for item in items {
        if let openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            summary,
            content,
            ..
        }) = item
        {
            pending_reasoning.extend(summary.into_iter().map(|part| part.text));
            pending_reasoning.extend(content.into_iter().flatten().map(|part| part.text));
            continue;
        }

        let Some(mut message) = response_item_to_chat_message(item) else {
            continue;
        };

        if let Some(reasoning) = non_empty_joined(std::mem::take(&mut pending_reasoning))
            && !attach_reasoning(&mut message, &reasoning)
        {
            messages.push(reasoning_to_chat_message(reasoning));
        }

        messages.push(message);
    }

    if let Some(reasoning) = non_empty_joined(pending_reasoning) {
        messages.push(reasoning_to_chat_message(reasoning));
    }

    messages
}

fn response_item_to_chat_message(
    item: openai::ResponseItem,
) -> Option<openai::ChatCompletionMessageParam> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(message)) => {
            easy_message_to_chat_message(message)
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Input(message)) => {
            input_message_to_chat_message(message)
        }
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            output_message_to_chat_param(message)
        }
        openai::ResponseItem::Typed(item) => typed_item_to_chat_message(item),
        openai::ResponseItem::Unknown(_) => None,
    }
}

fn easy_message_to_chat_message(
    message: openai::ResponseEasyInputMessageItem,
) -> Option<openai::ChatCompletionMessageParam> {
    Some(match message.role {
        openai::ResponseEasyInputMessageRole::Developer => {
            openai::ChatCompletionMessageParam::Developer {
                content: easy_input_content_to_chat_text_content(message.content),
                name: None,
                extra: Default::default(),
            }
        }
        openai::ResponseEasyInputMessageRole::System => {
            openai::ChatCompletionMessageParam::System {
                content: easy_input_content_to_chat_text_content(message.content),
                name: None,
                extra: Default::default(),
            }
        }
        openai::ResponseEasyInputMessageRole::User => openai::ChatCompletionMessageParam::User {
            content: easy_input_content_to_chat_content(message.content),
            name: None,
            extra: Default::default(),
        },
        openai::ResponseEasyInputMessageRole::Assistant => {
            openai::ChatCompletionMessageParam::Assistant {
                content: Some(easy_input_content_to_chat_assistant_content(
                    message.content,
                )),
                audio: None,
                function_call: None,
                name: None,
                reasoning_content: None,
                refusal: None,
                tool_calls: None,
                extra: Default::default(),
            }
        }
    })
}

fn easy_input_content_to_chat_content(
    content: openai::ResponseEasyInputContent,
) -> openai::ChatContent {
    match content {
        openai::ResponseEasyInputContent::Text(text) => openai::ChatContent::Text(text),
        openai::ResponseEasyInputContent::Parts(parts) => {
            response_input_parts_to_chat_content(parts)
        }
    }
}

fn easy_input_content_to_chat_text_content(
    content: openai::ResponseEasyInputContent,
) -> openai::ChatTextContent {
    match content {
        openai::ResponseEasyInputContent::Text(text) => openai::ChatTextContent::Text(text),
        openai::ResponseEasyInputContent::Parts(parts) => {
            response_input_parts_to_chat_text_content(parts)
        }
    }
}

fn easy_input_content_to_chat_assistant_content(
    content: openai::ResponseEasyInputContent,
) -> openai::ChatAssistantContent {
    match content {
        openai::ResponseEasyInputContent::Text(text) => openai::ChatAssistantContent::Text(text),
        openai::ResponseEasyInputContent::Parts(parts) => openai::ChatAssistantContent::Parts(
            parts
                .into_iter()
                .filter_map(|part| match part {
                    openai::ResponseInputContentPart::InputText {
                        text,
                        prompt_cache_breakpoint,
                        ..
                    } => Some(openai::ChatAssistantContentPart::Text {
                        text,
                        prompt_cache_breakpoint,
                        extra: Default::default(),
                    }),
                    other => {
                        warn_dropped_response_breakpoint(&other, "OpenAI Chat assistant message");
                        None
                    }
                })
                .collect(),
        ),
    }
}

fn input_message_to_chat_message(
    message: openai::ResponseInputMessageItem,
) -> Option<openai::ChatCompletionMessageParam> {
    Some(match message.role {
        openai::ResponseInputMessageRole::Developer => {
            openai::ChatCompletionMessageParam::Developer {
                content: response_input_parts_to_chat_text_content(message.content),
                name: None,
                extra: Default::default(),
            }
        }
        openai::ResponseInputMessageRole::System => openai::ChatCompletionMessageParam::System {
            content: response_input_parts_to_chat_text_content(message.content),
            name: None,
            extra: Default::default(),
        },
        openai::ResponseInputMessageRole::User => openai::ChatCompletionMessageParam::User {
            content: response_input_parts_to_chat_content(message.content),
            name: None,
            extra: Default::default(),
        },
    })
}

fn output_message_to_chat_param(
    message: openai::ResponseOutputMessageItem,
) -> Option<openai::ChatCompletionMessageParam> {
    let mut parts = Vec::new();
    let mut refusal = None;
    for part in message.content {
        match part {
            openai::ResponseMessageOutputContentPart::OutputText { text, .. } => {
                parts.push(openai::ChatAssistantContentPart::Text {
                    text,
                    prompt_cache_breakpoint: None,
                    extra: Default::default(),
                });
            }
            openai::ResponseMessageOutputContentPart::Refusal { refusal: value, .. } => {
                refusal = Some(value.clone());
                parts.push(openai::ChatAssistantContentPart::Refusal {
                    refusal: value,
                    prompt_cache_breakpoint: None,
                    extra: Default::default(),
                });
            }
        }
    }

    Some(openai::ChatCompletionMessageParam::Assistant {
        content: (!parts.is_empty()).then_some(openai::ChatAssistantContent::Parts(parts)),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal,
        tool_calls: None,
        extra: Default::default(),
    })
}

fn typed_item_to_chat_message(
    item: openai::TypedResponseItem,
) -> Option<openai::ChatCompletionMessageParam> {
    match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            ..
        } => Some(openai::ChatCompletionMessageParam::Assistant {
            content: None,
            audio: None,
            function_call: None,
            name: None,
            reasoning_content: None,
            refusal: None,
            tool_calls: Some(vec![function_call_to_chat_tool_call(
                call_id, name, arguments,
            )]),
            extra: Default::default(),
        }),
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            ..
        } => Some(openai::ChatCompletionMessageParam::Assistant {
            content: None,
            audio: None,
            function_call: None,
            name: None,
            reasoning_content: None,
            refusal: None,
            tool_calls: Some(vec![custom_call_to_chat_tool_call(call_id, name, input)]),
            extra: Default::default(),
        }),
        openai::TypedResponseItem::ApplyPatchCall {
            call_id, operation, ..
        } => Some(openai::ChatCompletionMessageParam::Assistant {
            content: None,
            audio: None,
            function_call: None,
            name: None,
            reasoning_content: None,
            refusal: None,
            tool_calls: Some(vec![function_call_to_chat_tool_call(
                call_id,
                "apply_patch".to_owned(),
                serde_json::to_string(&operation).unwrap_or_else(|_| "{}".to_owned()),
            )]),
            extra: Default::default(),
        }),
        openai::TypedResponseItem::FunctionCallOutput {
            call_id, output, ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => Some(openai::ChatCompletionMessageParam::Tool {
            content: response_output_to_chat_content(output),
            tool_call_id: call_id,
            extra: Default::default(),
        }),
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id, output, ..
        } => Some(openai::ChatCompletionMessageParam::Tool {
            content: openai::ChatTextContent::Text(output.unwrap_or_default()),
            tool_call_id: call_id,
            extra: Default::default(),
        }),
        openai::TypedResponseItem::Reasoning { .. } => None,
        _ => None,
    }
}

fn attach_reasoning(message: &mut openai::ChatCompletionMessageParam, reasoning: &str) -> bool {
    let openai::ChatCompletionMessageParam::Assistant {
        reasoning_content, ..
    } = message
    else {
        return false;
    };

    match reasoning_content {
        Some(existing) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(reasoning);
        }
        _ => *reasoning_content = Some(reasoning.to_owned()),
    }
    true
}

fn reasoning_to_chat_message(reasoning: String) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::Assistant {
        content: None,
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: Some(reasoning),
        refusal: None,
        tool_calls: None,
        extra: Default::default(),
    }
}

fn non_empty_joined(parts: Vec<String>) -> Option<String> {
    let joined = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!joined.is_empty()).then_some(joined)
}

fn response_input_parts_to_chat_content(
    parts: Vec<openai::ResponseInputContentPart>,
) -> openai::ChatContent {
    let parts = parts
        .into_iter()
        .filter_map(response_input_part_to_chat_part)
        .collect::<Vec<_>>();

    if parts.len() == 1 {
        match parts.into_iter().next() {
            Some(openai::ChatContentPart::Text {
                text,
                prompt_cache_breakpoint: None,
                ..
            }) => openai::ChatContent::Text(text),
            Some(part) => openai::ChatContent::Parts(vec![part]),
            None => openai::ChatContent::Parts(Vec::new()),
        }
    } else {
        openai::ChatContent::Parts(parts)
    }
}

fn response_input_part_to_chat_part(
    part: openai::ResponseInputContentPart,
) -> Option<openai::ChatContentPart> {
    match part {
        openai::ResponseInputContentPart::InputText {
            text,
            prompt_cache_breakpoint,
            ..
        } => Some(openai::ChatContentPart::Text {
            text,
            prompt_cache_breakpoint,
            extra: Default::default(),
        }),
        openai::ResponseInputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            prompt_cache_breakpoint,
            ..
        } => {
            if let Some(url) = image_url {
                Some(openai::ChatContentPart::ImageUrl {
                    image_url: openai::ImageUrl {
                        url,
                        detail: detail.and_then(response_detail_to_chat_detail),
                        extra: Default::default(),
                    },
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                })
            } else if let Some(file_id) = file_id {
                Some(openai::ChatContentPart::File {
                    file: openai::ChatFileRef {
                        file_data: None,
                        file_id: Some(file_id),
                        filename: None,
                        extra: Default::default(),
                    },
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                })
            } else {
                if prompt_cache_breakpoint.is_some() {
                    tracing::warn!(
                        block_type = "input_image",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
                None
            }
        }
        openai::ResponseInputContentPart::InputAudio { input_audio, .. } => {
            Some(openai::ChatContentPart::InputAudio {
                input_audio: openai::InputAudio {
                    data: input_audio.data,
                    format: input_audio.format,
                    extra: Default::default(),
                },
                prompt_cache_breakpoint: None,
                extra: Default::default(),
            })
        }
        openai::ResponseInputContentPart::InputFile {
            file_data,
            file_id,
            filename,
            prompt_cache_breakpoint,
            ..
        } => Some(openai::ChatContentPart::File {
            file: openai::ChatFileRef {
                file_data,
                file_id,
                filename,
                extra: Default::default(),
            },
            prompt_cache_breakpoint,
            extra: Default::default(),
        }),
    }
}

fn response_input_parts_to_chat_text_content(
    parts: Vec<openai::ResponseInputContentPart>,
) -> openai::ChatTextContent {
    openai::ChatTextContent::Parts(
        parts
            .into_iter()
            .filter_map(|part| match part {
                openai::ResponseInputContentPart::InputText {
                    text,
                    prompt_cache_breakpoint,
                    ..
                } => Some(openai::ChatTextContentPart::Text {
                    text,
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                }),
                other => {
                    warn_dropped_response_breakpoint(&other, "OpenAI Chat text message");
                    None
                }
            })
            .collect(),
    )
}

fn warn_dropped_response_breakpoint(part: &openai::ResponseInputContentPart, target: &str) {
    let (block_type, has_breakpoint) = match part {
        openai::ResponseInputContentPart::InputText {
            prompt_cache_breakpoint,
            ..
        } => ("input_text", prompt_cache_breakpoint.is_some()),
        openai::ResponseInputContentPart::InputImage {
            prompt_cache_breakpoint,
            ..
        } => ("input_image", prompt_cache_breakpoint.is_some()),
        openai::ResponseInputContentPart::InputFile {
            prompt_cache_breakpoint,
            ..
        } => ("input_file", prompt_cache_breakpoint.is_some()),
        openai::ResponseInputContentPart::InputAudio { .. } => ("input_audio", false),
    };
    if has_breakpoint {
        tracing::warn!(
            block_type,
            conversion_target = target,
            "cache breakpoint dropped during protocol conversion"
        );
    }
}
