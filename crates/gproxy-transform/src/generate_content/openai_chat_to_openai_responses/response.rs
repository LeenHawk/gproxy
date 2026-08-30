use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut refusal = String::new();
    let mut annotations = Vec::new();
    let mut calls = Vec::new();
    let mut raw = Vec::new();
    let mut message_rest: openai::Rest = Default::default();
    for item in input.output {
        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                message_rest.extend(message.rest);
                if let Some(phase) = message.phase {
                    message_rest.insert(
                        "responses_message_phase".into(),
                        serde_json::to_value(phase)?,
                    );
                }
                for part in message.content {
                    match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            text.push(part.text);
                            annotations
                                .extend(part.annotations.into_iter().filter_map(chat_annotation));
                            message_rest.extend(part.rest);
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            text.push(part.refusal.clone());
                            refusal.push_str(&part.refusal);
                            message_rest.extend(part.rest);
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(value) => raw.push(value),
                    }
                }
            }
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    arguments,
                    call_id,
                    name,
                    id,
                    mut rest,
                    caller,
                    namespace,
                    status,
                } => {
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "namespace", namespace)?;
                    preserve_option(&mut rest, "status", status)?;
                    calls.push(openai::ChatToolCall::Function(
                        openai::ChatFunctionToolCall {
                            id: call_id,
                            type_: openai::FunctionToolChoiceType::Function,
                            function: openai::FunctionCall {
                                arguments,
                                name,
                                rest: Default::default(),
                            },
                            rest,
                        },
                    ))
                }
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input,
                    name,
                    id,
                    mut rest,
                    caller,
                    namespace,
                } => {
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "namespace", namespace)?;
                    calls.push(openai::ChatToolCall::Custom(openai::ChatCustomToolCall {
                        id: call_id,
                        type_: openai::CustomToolChoiceType::Custom,
                        custom: openai::CustomToolCall {
                            input,
                            name,
                            rest: Default::default(),
                        },
                        rest,
                    }))
                }
                openai::TypedResponseItem::ShellCall {
                    action,
                    call_id,
                    id,
                    caller,
                    environment,
                    status,
                    created_by,
                    mut rest,
                } => {
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "environment", environment)?;
                    preserve_option(&mut rest, "status", status)?;
                    preserve_option(&mut rest, "created_by", created_by)?;
                    calls.push(function_call(
                        call_id,
                        "shell",
                        serde_json::to_string(&action)?,
                        rest,
                    ));
                }
                openai::TypedResponseItem::ApplyPatchCall {
                    call_id,
                    operation,
                    status,
                    id,
                    caller,
                    created_by,
                    mut rest,
                } => {
                    preserve_option(&mut rest, "responses_item_id", id)?;
                    preserve_option(&mut rest, "caller", caller)?;
                    preserve_option(&mut rest, "status", Some(status))?;
                    preserve_option(&mut rest, "created_by", created_by)?;
                    calls.push(function_call(
                        call_id,
                        "apply_patch",
                        serde_json::to_string(&operation)?,
                        rest,
                    ));
                }
                openai::TypedResponseItem::Reasoning {
                    summary,
                    content,
                    encrypted_content,
                    status,
                    rest,
                    ..
                } => {
                    reasoning.extend(summary.into_iter().map(|part| part.text));
                    reasoning.extend(content.into_iter().flatten().map(|part| part.text));
                    message_rest.extend(rest);
                    preserve_option(
                        &mut message_rest,
                        "responses_reasoning_encrypted_content",
                        encrypted_content,
                    )?;
                    preserve_option(&mut message_rest, "responses_reasoning_status", status)?;
                }
                other => raw.push(serde_json::to_value(other)?),
            },
            other => raw.push(serde_json::to_value(other)?),
        }
    }
    let has_calls = !calls.is_empty();
    if text.is_empty()
        && let Some(fallback) = input.output_text.clone().filter(|value| !value.is_empty())
    {
        text.push(fallback);
    }
    if !raw.is_empty() {
        message_rest.insert(
            "responses_output_items".into(),
            serde_json::Value::Array(raw),
        );
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
    let output = openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![openai::ChatCompletionChoice {
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
                rest: message_rest,
            },
            rest: Default::default(),
        }],
        created: input.created_at,
        model: input
            .model
            .ok_or_else(|| TransformError::shape("Responses response", "model missing"))?,
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier: input.service_tier,
        system_fingerprint: None,
        usage: input.usage.map(usage::responses_to_chat),
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
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
        } => Some(openai::ChatAnnotation {
            type_: openai::ChatAnnotationType::UrlCitation,
            url_citation: openai::UrlCitation {
                end_index,
                start_index,
                title,
                url,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        _ => None,
    }
}

fn function_call(
    id: String,
    name: &str,
    arguments: String,
    rest: openai::Rest,
) -> openai::ChatToolCall {
    openai::ChatToolCall::Function(openai::ChatFunctionToolCall {
        id,
        type_: openai::FunctionToolChoiceType::Function,
        function: openai::FunctionCall {
            arguments,
            name: name.into(),
            rest: Default::default(),
        },
        rest,
    })
}

fn preserve_option<T: serde::Serialize>(
    rest: &mut openai::Rest,
    key: &str,
    value: Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        rest.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}
