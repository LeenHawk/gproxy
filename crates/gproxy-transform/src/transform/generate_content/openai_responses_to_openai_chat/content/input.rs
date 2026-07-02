use crate::protocol::openai;

use super::util::{response_detail_to_chat_detail, response_output_to_text};
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
        Some(openai::ResponseInput::Items(items)) => items
            .into_iter()
            .filter_map(response_item_to_chat_message)
            .collect(),
        None => Vec::new(),
    }
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
                content: openai::ChatTextContent::Text(easy_input_content_to_text(message.content)),
                name: None,
                extra: Default::default(),
            }
        }
        openai::ResponseEasyInputMessageRole::System => {
            openai::ChatCompletionMessageParam::System {
                content: openai::ChatTextContent::Text(easy_input_content_to_text(message.content)),
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
                content: Some(openai::ChatAssistantContent::Text(
                    easy_input_content_to_text(message.content),
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

fn easy_input_content_to_text(content: openai::ResponseEasyInputContent) -> String {
    match content {
        openai::ResponseEasyInputContent::Text(text) => text,
        openai::ResponseEasyInputContent::Parts(parts) => response_input_parts_to_text(parts),
    }
}

fn input_message_to_chat_message(
    message: openai::ResponseInputMessageItem,
) -> Option<openai::ChatCompletionMessageParam> {
    Some(match message.role {
        openai::ResponseInputMessageRole::Developer => {
            openai::ChatCompletionMessageParam::Developer {
                content: openai::ChatTextContent::Text(response_input_parts_to_text(
                    message.content,
                )),
                name: None,
                extra: Default::default(),
            }
        }
        openai::ResponseInputMessageRole::System => openai::ChatCompletionMessageParam::System {
            content: openai::ChatTextContent::Text(response_input_parts_to_text(message.content)),
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
                    extra: Default::default(),
                });
            }
            openai::ResponseMessageOutputContentPart::Refusal { refusal: value, .. } => {
                refusal = Some(value.clone());
                parts.push(openai::ChatAssistantContentPart::Refusal {
                    refusal: value,
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
            content: openai::ChatTextContent::Text(response_output_to_text(output)),
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

fn response_input_parts_to_chat_content(
    parts: Vec<openai::ResponseInputContentPart>,
) -> openai::ChatContent {
    let parts = parts
        .into_iter()
        .filter_map(response_input_part_to_chat_part)
        .collect::<Vec<_>>();

    if parts.len() == 1 {
        match parts.into_iter().next() {
            Some(openai::ChatContentPart::Text { text, .. }) => openai::ChatContent::Text(text),
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
        openai::ResponseInputContentPart::InputText { text, .. } => {
            Some(openai::ChatContentPart::Text {
                text,
                extra: Default::default(),
            })
        }
        openai::ResponseInputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            ..
        } => image_url
            .map(|url| openai::ChatContentPart::ImageUrl {
                image_url: openai::ImageUrl {
                    url,
                    detail: detail.and_then(response_detail_to_chat_detail),
                    extra: Default::default(),
                },
                extra: Default::default(),
            })
            .or_else(|| {
                file_id.map(|file_id| openai::ChatContentPart::File {
                    file: openai::ChatFileRef {
                        file_data: None,
                        file_id: Some(file_id),
                        filename: None,
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                })
            }),
        openai::ResponseInputContentPart::InputAudio { input_audio, .. } => {
            Some(openai::ChatContentPart::InputAudio {
                input_audio: openai::InputAudio {
                    data: input_audio.data,
                    format: input_audio.format,
                    extra: Default::default(),
                },
                extra: Default::default(),
            })
        }
        openai::ResponseInputContentPart::InputFile {
            file_data,
            file_id,
            filename,
            ..
        } => Some(openai::ChatContentPart::File {
            file: openai::ChatFileRef {
                file_data,
                file_id,
                filename,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
    }
}

fn response_input_parts_to_text(parts: Vec<openai::ResponseInputContentPart>) -> String {
    parts
        .into_iter()
        .filter_map(|part| match part {
            openai::ResponseInputContentPart::InputText { text, .. } => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
