use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ResponseObject,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut refusal = String::new();
    let mut annotations = Vec::new();
    let mut calls = Vec::new();
    for item in input.output {
        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                for part in message.content {
                    match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            text.push(part.text);
                            annotations
                                .extend(part.annotations.into_iter().filter_map(chat_annotation));
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            text.push(part.refusal.clone());
                            refusal.push_str(&part.refusal);
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(_) => {}
                        #[cfg(not(feature = "exhaustive"))]
                        _ => {
                            return Err(crate::TransformError::unsupported(
                                "protocol enum",
                                "unrecognized external variant",
                            ));
                        }
                    }
                }
            }
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    arguments,
                    call_id,
                    name,
                    ..
                } => calls.push(openai::ChatToolCall::Function(crate::wire!(
                    openai::ChatFunctionToolCall {
                        id: call_id,
                        type_: openai::FunctionToolChoiceType::Function,
                        function: openai::FunctionCall {
                            arguments,
                            name,
                            rest: Default::default(),
                        },
                        rest: Default::default(),
                    }
                ))),
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input,
                    name,
                    ..
                } => calls.push(openai::ChatToolCall::Custom(crate::wire!(
                    openai::ChatCustomToolCall {
                        id: call_id,
                        type_: openai::CustomToolChoiceType::Custom,
                        custom: openai::CustomToolCall {
                            input,
                            name,
                            rest: Default::default(),
                        },
                        rest: Default::default(),
                    }
                ))),
                openai::TypedResponseItem::ShellCall {
                    action, call_id, ..
                } => {
                    calls.push(function_call(
                        call_id,
                        "shell",
                        serde_json::to_string(&action)?,
                    ));
                }
                openai::TypedResponseItem::ApplyPatchCall {
                    call_id, operation, ..
                } => {
                    calls.push(function_call(
                        call_id,
                        "apply_patch",
                        serde_json::to_string(&operation)?,
                    ));
                }
                openai::TypedResponseItem::Reasoning {
                    summary, content, ..
                } => {
                    reasoning.extend(summary.into_iter().map(|part| part.text));
                    reasoning.extend(content.into_iter().flatten().map(|part| part.text));
                }
                _ => {}
            },
            _ => {}
        }
    }
    let has_calls = !calls.is_empty();
    if text.is_empty()
        && let Some(fallback) = input.output_text.clone().filter(|value| !value.is_empty())
    {
        text.push(fallback);
    }
    let finish_reason = if has_calls {
        openai::ChatFinishReason::ToolCalls
    } else if matches!(input.status, Some(openai::ResponseStatus::Incomplete)) {
        openai::ChatFinishReason::Length
    } else if matches!(input.status, Some(openai::ResponseStatus::Failed)) {
        openai::ChatFinishReason::ContentFilter
    } else {
        openai::ChatFinishReason::Stop
    };
    let output = crate::wire!(openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![crate::wire!(openai::ChatCompletionChoice {
            finish_reason,
            index: 0,
            logprobs: None,
            message: openai::ChatMessage {
                role: openai::ChatCompletionMessageRole::Assistant,
                content: joined(text),
                refusal: (!refusal.is_empty()).then_some(refusal),
                annotations: (!annotations.is_empty()).then_some(annotations),
                audio: None,
                function_call: None,
                reasoning_content: joined(reasoning),
                tool_calls: has_calls.then_some(calls),
                rest: Default::default(),
            },
            rest: Default::default(),
        })],
        created: input.created_at,
        model: input
            .model
            .ok_or_else(|| TransformError::shape("Responses response", "model missing"))?,
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier: input.service_tier,
        system_fingerprint: None,
        usage: input.usage.map(usage::responses_to_chat),
        rest: Default::default(),
    });
    Ok(output)
}

fn joined(parts: Vec<String>) -> Option<String> {
    let value = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!value.is_empty()).then_some(value)
}

fn chat_annotation(annotation: openai::ResponseAnnotation) -> Option<openai::ChatAnnotation> {
    match annotation {
        openai::ResponseAnnotation::UrlCitation {
            end_index,
            start_index,
            title,
            url,
            ..
        } => Some(crate::wire!(openai::ChatAnnotation {
            type_: openai::ChatAnnotationType::UrlCitation,
            url_citation: openai::UrlCitation {
                end_index,
                start_index,
                title,
                url,
                rest: Default::default(),
            },
            rest: Default::default(),
        })),
        _ => None,
    }
}

fn function_call(id: String, name: &str, arguments: String) -> openai::ChatToolCall {
    openai::ChatToolCall::Function(crate::wire!(openai::ChatFunctionToolCall {
        id,
        type_: openai::FunctionToolChoiceType::Function,
        function: openai::FunctionCall {
            arguments,
            name: name.into(),
            rest: Default::default(),
        },
        rest: Default::default(),
    }))
}
