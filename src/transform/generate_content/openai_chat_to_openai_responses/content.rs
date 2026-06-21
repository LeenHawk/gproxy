use std::collections::BTreeMap;

use crate::protocol::openai;

use super::super::common;
use super::tools::{
    ResponseToolOutputKind, chat_tool_call_to_response_item,
    chat_tool_call_to_response_item_and_output_kind, legacy_function_call_id,
    legacy_function_call_to_response_item, tool_output_item,
};

pub(super) fn chat_messages_to_response_items(
    messages: Vec<openai::ChatCompletionMessageParam>,
) -> Vec<openai::ResponseItem> {
    let mut tool_outputs = BTreeMap::new();
    let mut items = Vec::new();
    for (index, message) in messages.into_iter().enumerate() {
        items.extend(chat_message_to_response_items(
            index,
            message,
            &mut tool_outputs,
        ));
    }
    items
}

fn chat_message_to_response_items(
    index: usize,
    message: openai::ChatCompletionMessageParam,
    tool_outputs: &mut BTreeMap<String, (String, ResponseToolOutputKind)>,
) -> Vec<openai::ResponseItem> {
    match message {
        openai::ChatCompletionMessageParam::Developer { content, .. } => vec![easy_input(
            openai::ResponseEasyInputMessageRole::Developer,
            openai::ResponseEasyInputContent::Text(chat_text_content_to_text(content)),
        )],
        openai::ChatCompletionMessageParam::System { content, .. } => vec![easy_input(
            openai::ResponseEasyInputMessageRole::System,
            openai::ResponseEasyInputContent::Text(chat_text_content_to_text(content)),
        )],
        openai::ChatCompletionMessageParam::User { content, .. } => vec![easy_input(
            openai::ResponseEasyInputMessageRole::User,
            chat_content_to_easy_content(content),
        )],
        openai::ChatCompletionMessageParam::Assistant {
            content,
            function_call,
            refusal,
            tool_calls,
            ..
        } => {
            let mut items = Vec::new();
            if let Some(content) = content {
                items.push(output_message_item(
                    format!("msg_{index}"),
                    chat_assistant_content_to_output_parts(content, refusal),
                ));
            } else if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
                items.push(output_message_item(
                    format!("msg_{index}"),
                    vec![openai::ResponseMessageOutputContentPart::Refusal {
                        refusal,
                        extra: Default::default(),
                    }],
                ));
            }
            if let Some(function_call) = function_call {
                let call_id = legacy_function_call_id(&function_call.name);
                tool_outputs.insert(call_id.clone(), (call_id, ResponseToolOutputKind::Function));
                items.push(legacy_function_call_to_response_item(function_call));
            }
            if let Some(tool_calls) = tool_calls {
                for call in tool_calls {
                    let (item, original_call_id, response_call_id, kind) =
                        chat_tool_call_to_response_item_and_output_kind(call);
                    tool_outputs.insert(original_call_id, (response_call_id, kind));
                    items.push(item);
                }
            }
            if items.is_empty() {
                items.push(easy_input(
                    openai::ResponseEasyInputMessageRole::Assistant,
                    openai::ResponseEasyInputContent::Text(String::new()),
                ));
            }
            items
        }
        openai::ChatCompletionMessageParam::Tool {
            content,
            tool_call_id,
            ..
        } => {
            let kind = tool_outputs.get(&tool_call_id).cloned().unwrap_or_else(|| {
                (
                    common::response_call_id(&tool_call_id),
                    ResponseToolOutputKind::Function,
                )
            });
            vec![tool_output_item(
                kind.1,
                kind.0,
                openai::ResponseOutput::Text(chat_text_content_to_text(content)),
            )]
        }
        openai::ChatCompletionMessageParam::Function { content, name, .. } => {
            vec![tool_output_item(
                ResponseToolOutputKind::Function,
                legacy_function_call_id(&name),
                openai::ResponseOutput::Text(content),
            )]
        }
    }
}

pub(super) fn chat_message_to_response_output_items(
    index: u32,
    message: openai::ChatMessage,
) -> Vec<openai::ResponseOutputItem> {
    let mut items = Vec::new();
    let mut parts = Vec::new();

    if let Some(content) = message.content.filter(|value| !value.is_empty()) {
        parts.push(openai::ResponseMessageOutputContentPart::OutputText {
            annotations: message
                .annotations
                .unwrap_or_default()
                .into_iter()
                .map(chat_annotation_to_response_annotation)
                .collect(),
            logprobs: None,
            text: content,
            extra: Default::default(),
        });
    }
    if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
        parts.push(openai::ResponseMessageOutputContentPart::Refusal {
            refusal,
            extra: Default::default(),
        });
    }
    if !parts.is_empty() {
        items.push(openai::ResponseOutputItem(output_message_item(
            format!("msg_{index}"),
            parts,
        )));
    }
    if let Some(function_call) = message.function_call {
        items.push(openai::ResponseOutputItem(
            legacy_function_call_to_response_item(function_call),
        ));
    }
    if let Some(tool_calls) = message.tool_calls {
        items.extend(
            tool_calls
                .into_iter()
                .map(chat_tool_call_to_response_item)
                .map(openai::ResponseOutputItem),
        );
    }
    items
}

fn easy_input(
    role: openai::ResponseEasyInputMessageRole,
    content: openai::ResponseEasyInputContent,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
        openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content,
            phase: None,
            extra: Default::default(),
        },
    ))
}

fn output_message_item(
    id: String,
    content: Vec<openai::ResponseMessageOutputContentPart>,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(
        openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id,
            role: openai::ResponseOutputMessageRole::Assistant,
            content,
            status: openai::ResponseItemLifecycleStatus::Completed,
            phase: None,
            extra: Default::default(),
        },
    ))
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

fn chat_assistant_content_to_output_parts(
    content: openai::ChatAssistantContent,
    refusal: Option<String>,
) -> Vec<openai::ResponseMessageOutputContentPart> {
    let mut parts = match content {
        openai::ChatAssistantContent::Text(text) => {
            vec![openai::ResponseMessageOutputContentPart::OutputText {
                annotations: Vec::new(),
                logprobs: None,
                text,
                extra: Default::default(),
            }]
        }
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatAssistantContentPart::Text { text, .. } => {
                    openai::ResponseMessageOutputContentPart::OutputText {
                        annotations: Vec::new(),
                        logprobs: None,
                        text,
                        extra: Default::default(),
                    }
                }
                openai::ChatAssistantContentPart::Refusal { refusal, .. } => {
                    openai::ResponseMessageOutputContentPart::Refusal {
                        refusal,
                        extra: Default::default(),
                    }
                }
            })
            .collect(),
    };
    if let Some(refusal) = refusal.filter(|value| !value.is_empty()) {
        parts.push(openai::ResponseMessageOutputContentPart::Refusal {
            refusal,
            extra: Default::default(),
        });
    }
    parts
}

fn chat_content_to_easy_content(content: openai::ChatContent) -> openai::ResponseEasyInputContent {
    match content {
        openai::ChatContent::Text(text) => openai::ResponseEasyInputContent::Text(text),
        openai::ChatContent::Parts(parts) => openai::ResponseEasyInputContent::Parts(
            parts.into_iter().map(chat_part_to_response_part).collect(),
        ),
    }
}

fn chat_part_to_response_part(part: openai::ChatContentPart) -> openai::ResponseInputContentPart {
    match part {
        openai::ChatContentPart::Text { text, .. } => openai::ResponseInputContentPart::InputText {
            text,
            extra: Default::default(),
        },
        openai::ChatContentPart::ImageUrl { image_url, .. } => {
            openai::ResponseInputContentPart::InputImage {
                detail: image_url.detail.map(chat_detail_to_response_detail),
                file_id: None,
                image_url: Some(image_url.url),
                extra: Default::default(),
            }
        }
        openai::ChatContentPart::InputAudio { input_audio, .. } => {
            openai::ResponseInputContentPart::InputAudio {
                input_audio: openai::InputAudioContent {
                    data: input_audio.data,
                    format: input_audio.format,
                    extra: Default::default(),
                },
                extra: Default::default(),
            }
        }
        openai::ChatContentPart::File { file, .. } => openai::ResponseInputContentPart::InputFile {
            detail: None,
            file_data: file.file_data,
            file_id: file.file_id,
            file_url: None,
            filename: file.filename,
            extra: Default::default(),
        },
    }
}

fn chat_detail_to_response_detail(detail: openai::ChatImageDetailLevel) -> openai::DetailLevel {
    match detail {
        openai::ChatImageDetailLevel::Auto => openai::DetailLevel::Auto,
        openai::ChatImageDetailLevel::Low => openai::DetailLevel::Low,
        openai::ChatImageDetailLevel::High => openai::DetailLevel::High,
    }
}

fn chat_annotation_to_response_annotation(
    annotation: openai::ChatAnnotation,
) -> openai::ResponseAnnotation {
    openai::ResponseAnnotation::UrlCitation {
        end_index: annotation.url_citation.end_index,
        start_index: annotation.url_citation.start_index,
        title: annotation.url_citation.title,
        url: annotation.url_citation.url,
        extra: Default::default(),
    }
}
