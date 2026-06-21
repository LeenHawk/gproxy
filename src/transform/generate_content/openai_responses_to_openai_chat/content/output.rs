use crate::protocol::openai;

use crate::transform::generate_content::openai_responses_to_openai_chat::tools::{
    custom_call_to_chat_tool_call, function_call_to_chat_tool_call,
};

pub(in crate::transform::generate_content::openai_responses_to_openai_chat) fn response_output_items_to_chat_message(
    items: Vec<openai::ResponseOutputItem>,
    fallback_text: Option<String>,
) -> openai::ChatMessage {
    let mut text_parts = Vec::new();
    let mut refusal = None;
    let mut annotations = Vec::new();
    let mut tool_calls = Vec::new();
    let mut reasoning_parts = Vec::new();

    for item in items {
        match item.0 {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                for part in message.content {
                    match part {
                        openai::ResponseMessageOutputContentPart::OutputText {
                            text,
                            annotations: part_annotations,
                            ..
                        } => {
                            text_parts.push(text);
                            annotations.extend(
                                part_annotations
                                    .into_iter()
                                    .filter_map(response_annotation_to_chat_annotation),
                            );
                        }
                        openai::ResponseMessageOutputContentPart::Refusal {
                            refusal: value,
                            ..
                        } => {
                            refusal = Some(value.clone());
                            text_parts.push(value);
                        }
                    }
                }
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                ..
            }) => {
                tool_calls.push(function_call_to_chat_tool_call(call_id, name, arguments));
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                call_id,
                input,
                name,
                ..
            }) => {
                tool_calls.push(custom_call_to_chat_tool_call(call_id, name, input));
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
                summary,
                content,
                ..
            }) => {
                // Mirror the streaming path, which surfaces both reasoning
                // summary and reasoning text as chat `reasoning_content`.
                reasoning_parts.extend(summary.into_iter().map(|part| part.text));
                reasoning_parts.extend(content.into_iter().flatten().map(|part| part.text));
            }
            _ => {}
        }
    }

    if text_parts.is_empty()
        && let Some(text) = fallback_text.filter(|value| !value.is_empty())
    {
        text_parts.push(text);
    }

    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: (!text_parts.is_empty()).then(|| text_parts.join("\n")),
        refusal,
        annotations: (!annotations.is_empty()).then_some(annotations),
        audio: None,
        function_call: None,
        reasoning_content: non_empty_joined(reasoning_parts),
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
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

fn response_annotation_to_chat_annotation(
    annotation: openai::ResponseAnnotation,
) -> Option<openai::ChatAnnotation> {
    match annotation {
        openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            ..
        } => Some(openai::ChatAnnotation {
            type_: openai::ChatAnnotationType::UrlCitation,
            url_citation: openai::UrlCitation {
                end_index,
                start_index,
                title,
                url,
                extra: Default::default(),
            },
            extra: Default::default(),
        }),
        _ => None,
    }
}
