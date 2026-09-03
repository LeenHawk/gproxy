//! Synthesize strict typed stream events from complete responses.

use gproxy_protocol::{claude, gemini, openai};

pub fn openai_chat(response: openai::ChatCompletionResponse) -> Vec<openai::ChatCompletionChunk> {
    let choices = response
        .choices
        .into_iter()
        .map(|choice| {
            let message = choice.message;
            crate::wire!(openai::ChatChunkChoice {
                index: choice.index,
                delta: crate::wire!(openai::ChatDelta {
                    role: Some(openai::ChatDeltaRole::Assistant),
                    content: message.content,
                    reasoning_content: message.reasoning_content,
                    refusal: message.refusal,
                    tool_calls: message.tool_calls.map(chat_tool_deltas),
                    function_call: message.function_call.map(|call| {
                        crate::wire!(openai::FunctionCallDelta {
                            arguments: Some(call.arguments),
                            name: Some(call.name),
                            rest: Default::default(),
                        })
                    }),
                    obfuscation: None,
                    rest: Default::default(),
                }),
                finish_reason: Some(choice.finish_reason),
                logprobs: choice.logprobs,
                rest: Default::default(),
            })
        })
        .collect();
    vec![crate::wire!(openai::ChatCompletionChunk {
        id: response.id,
        choices,
        created: response.created,
        model: response.model,
        object: openai::ChatCompletionChunkObjectType::ChatCompletionChunk,
        service_tier: response.service_tier,
        system_fingerprint: response.system_fingerprint,
        usage: response.usage,
        rest: Default::default(),
    })]
}

fn chat_tool_deltas(calls: Vec<openai::ChatToolCall>) -> Vec<openai::ChatToolCallDelta> {
    calls
        .into_iter()
        .enumerate()
        .filter_map(|(index, call)| {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            match call {
                openai::ChatToolCall::Function(call) => {
                    Some(crate::wire!(openai::ChatToolCallDelta {
                        index,
                        id: Some(call.id),
                        type_: Some(openai::ChatToolCallType::Function),
                        function: Some(crate::wire!(openai::FunctionCallDelta {
                            arguments: Some(call.function.arguments),
                            name: Some(call.function.name),
                            rest: Default::default(),
                        })),
                        custom: None,
                        rest: Default::default(),
                    }))
                }
                openai::ChatToolCall::Custom(call) => {
                    Some(crate::wire!(openai::ChatToolCallDelta {
                        index,
                        id: Some(call.id),
                        type_: Some(openai::ChatToolCallType::Custom),
                        function: None,
                        custom: Some(crate::wire!(openai::CustomToolCallDelta {
                            input: Some(call.custom.input),
                            name: Some(call.custom.name),
                            rest: Default::default(),
                        })),
                        rest: Default::default(),
                    }))
                }
                openai::ChatToolCall::Unknown(_) => None,
                #[cfg(not(feature = "exhaustive"))]
                _ => None,
            }
        })
        .collect()
}

pub fn gemini(response: gemini::GenerateContentResponse) -> Vec<gemini::GenerateContentResponse> {
    vec![response]
}

pub fn claude(response: claude::CreateMessageResponseBody) -> Vec<claude::StreamEvent> {
    let mut output = vec![claude_event(claude::KnownStreamEvent::MessageStart {
        message: Box::new(crate::wire!(claude::CreateMessageStartBody {
            id: response.id.clone(),
            type_: response.type_.clone(),
            role: response.role.clone(),
            content: Vec::new(),
            model: response.model.clone(),
            stop_reason: None,
            stop_sequence: None,
            usage: Some(response.usage.clone()),
            input_transformations: response.input_transformations.clone(),
            rest: Default::default(),
        })),
        rest: Default::default(),
    })];
    for (index, block) in response.content.into_iter().enumerate() {
        let index = u64::try_from(index).unwrap_or(u64::MAX);
        let (start, deltas) = claude_block(block);
        output.push(claude_event(claude::KnownStreamEvent::ContentBlockStart {
            index,
            content_block: Box::new(start),
            rest: Default::default(),
        }));
        output.extend(deltas.into_iter().map(|delta| {
            claude_event(claude::KnownStreamEvent::ContentBlockDelta {
                index,
                delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
                rest: Default::default(),
            })
        }));
        output.push(claude_event(claude::KnownStreamEvent::ContentBlockStop {
            index,
            rest: Default::default(),
        }));
    }
    output.push(claude_event(claude::KnownStreamEvent::MessageDelta {
        context_management: response.context_management.map(Box::new),
        delta: Box::new(crate::wire!(claude::MessageDelta {
            container: response.container,
            stop_reason: Some(response.stop_reason),
            stop_sequence: response.stop_sequence,
            stop_details: response.stop_details,
            rest: Default::default(),
        })),
        input_transformations: response.input_transformations,
        usage: Some(Box::new(response.usage)),
        rest: Default::default(),
    }));
    output.push(claude_event(claude::KnownStreamEvent::MessageStop {
        rest: Default::default(),
    }));
    output
}

fn claude_event(event: claude::KnownStreamEvent) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(event))
}

fn claude_block(
    block: claude::ContentBlock,
) -> (claude::ContentBlock, Vec<claude::KnownEventDelta>) {
    match block {
        claude::ResponseContentBlock::Text(mut block) => {
            let text = std::mem::take(&mut block.text);
            (
                claude::ResponseContentBlock::Text(block),
                vec![claude::KnownEventDelta::Text {
                    text,
                    rest: Default::default(),
                }],
            )
        }
        claude::ResponseContentBlock::Thinking(mut block) => {
            let thinking = std::mem::take(&mut block.thinking);
            let signature = block.signature.take();
            let mut deltas = vec![claude::KnownEventDelta::Thinking {
                estimated_tokens: None,
                thinking,
                rest: Default::default(),
            }];
            if let Some(signature) = signature {
                deltas.push(claude::KnownEventDelta::Signature {
                    signature,
                    rest: Default::default(),
                });
            }
            (claude::ResponseContentBlock::Thinking(block), deltas)
        }
        claude::ResponseContentBlock::ToolUse(mut block) => {
            let input = std::mem::take(&mut block.input);
            (
                claude::ResponseContentBlock::ToolUse(block),
                vec![claude::KnownEventDelta::InputJson {
                    partial_json: serde_json::to_string(&input).unwrap_or_else(|_| "{}".into()),
                    rest: Default::default(),
                }],
            )
        }
        other => (other, Vec::new()),
    }
}

pub fn openai_responses(response: openai::ResponseObject) -> Vec<openai::ResponseStreamEvent> {
    let mut sequence = 0_u64;
    let mut started = response.clone();
    started.status = Some(openai::ResponseStatus::InProgress);
    started.output.clear();
    started.output_text = None;
    started.usage = None;
    let mut output = vec![response_event(
        openai::KnownResponseStreamEvent::ResponseCreated(lifecycle(started, &mut sequence)),
    )];
    for (index, item) in response.output.iter().cloned().enumerate() {
        let output_index = u32::try_from(index).unwrap_or(u32::MAX);
        output.push(response_event(
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded(item_event(
                item.clone(),
                output_index,
                &mut sequence,
            )),
        ));
        output.extend(response_item_events(&item, output_index, &mut sequence));
        output.push(response_event(
            openai::KnownResponseStreamEvent::ResponseOutputItemDone(item_event(
                item,
                output_index,
                &mut sequence,
            )),
        ));
    }
    output.push(response_event(
        openai::KnownResponseStreamEvent::ResponseCompleted(lifecycle(response, &mut sequence)),
    ));
    output
}

fn response_event(event: openai::KnownResponseStreamEvent) -> openai::ResponseStreamEvent {
    openai::ResponseStreamEvent::Known(Box::new(event))
}

fn next(sequence: &mut u64) -> Option<u64> {
    let current = *sequence;
    *sequence = sequence.saturating_add(1);
    Some(current)
}

fn lifecycle(
    response: openai::ResponseObject,
    sequence: &mut u64,
) -> openai::ResponseLifecycleEvent {
    crate::wire!(openai::ResponseLifecycleEvent {
        response: Box::new(response),
        sequence_number: next(sequence),
        rest: Default::default(),
    })
}

fn item_event(
    item: openai::ResponseItem,
    output_index: u32,
    sequence: &mut u64,
) -> openai::ResponseOutputItemEvent {
    crate::wire!(openai::ResponseOutputItemEvent {
        item: Box::new(item),
        output_index,
        sequence_number: next(sequence),
        rest: Default::default(),
    })
}

fn response_item_events(
    item: &openai::ResponseItem,
    output_index: u32,
    sequence: &mut u64,
) -> Vec<openai::ResponseStreamEvent> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => message
            .content
            .iter()
            .enumerate()
            .flat_map(|(index, part)| {
                response_content_events(
                    &message.id,
                    output_index,
                    u32::try_from(index).unwrap_or(u32::MAX),
                    part,
                    sequence,
                )
            })
            .collect(),
        openai::ResponseItem::Typed(item) => response_tool_events(item, output_index, sequence),
        _ => Vec::new(),
    }
}

fn response_content_events(
    item_id: &str,
    output_index: u32,
    content_index: u32,
    part: &openai::ResponseMessageOutputContentPart,
    sequence: &mut u64,
) -> Vec<openai::ResponseStreamEvent> {
    match part {
        openai::ResponseMessageOutputContentPart::OutputText(part) => vec![
            response_event(openai::KnownResponseStreamEvent::ResponseOutputTextDelta(
                crate::wire!(openai::ResponseOutputTextDeltaEvent {
                    content_index: Some(content_index),
                    delta: part.text.clone(),
                    item_id: item_id.into(),
                    logprobs: None,
                    output_index,
                    sequence_number: next(sequence),
                    rest: Default::default(),
                }),
            )),
            response_event(openai::KnownResponseStreamEvent::ResponseOutputTextDone(
                crate::wire!(openai::ResponseOutputTextDoneEvent {
                    content_index,
                    item_id: item_id.into(),
                    logprobs: None,
                    output_index,
                    sequence_number: next(sequence),
                    text: part.text.clone(),
                    rest: Default::default(),
                }),
            )),
        ],
        openai::ResponseMessageOutputContentPart::Refusal(part) => vec![
            response_event(openai::KnownResponseStreamEvent::ResponseRefusalDelta(
                crate::wire!(openai::ResponseContentDeltaEvent {
                    content_index,
                    delta: part.refusal.clone(),
                    item_id: item_id.into(),
                    output_index,
                    sequence_number: next(sequence),
                    rest: Default::default(),
                }),
            )),
            response_event(openai::KnownResponseStreamEvent::ResponseRefusalDone(
                crate::wire!(openai::ResponseRefusalDoneEvent {
                    content_index,
                    item_id: item_id.into(),
                    output_index,
                    refusal: part.refusal.clone(),
                    sequence_number: next(sequence),
                    rest: Default::default(),
                }),
            )),
        ],
        _ => Vec::new(),
    }
}

fn response_tool_events(
    item: &openai::TypedResponseItem,
    output_index: u32,
    sequence: &mut u64,
) -> Vec<openai::ResponseStreamEvent> {
    match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            id,
            name,
            call_id,
            ..
        } => {
            let item_id = id.clone().unwrap_or_else(|| call_id.clone());
            vec![
                response_event(
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(
                        crate::wire!(openai::ResponseItemStringDeltaEvent {
                            delta: arguments.clone(),
                            item_id: item_id.clone(),
                            output_index,
                            sequence_number: next(sequence),
                            rest: Default::default(),
                        }),
                    ),
                ),
                response_event(
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(
                        crate::wire!(openai::ResponseFunctionCallArgumentsDoneEvent {
                            arguments: arguments.clone(),
                            item_id: Some(item_id),
                            name: Some(name.clone()),
                            output_index,
                            sequence_number: next(sequence),
                            rest: Default::default(),
                        }),
                    ),
                ),
            ]
        }
        openai::TypedResponseItem::CustomToolCall {
            call_id, id, input, ..
        } => {
            let item_id = id.clone().unwrap_or_else(|| call_id.clone());
            vec![
                response_event(
                    openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta(
                        crate::wire!(openai::ResponseItemStringDeltaEvent {
                            delta: input.clone(),
                            item_id: item_id.clone(),
                            output_index,
                            sequence_number: next(sequence),
                            rest: Default::default(),
                        }),
                    ),
                ),
                response_event(
                    openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(
                        crate::wire!(openai::ResponseCustomToolCallInputDoneEvent {
                            input: input.clone(),
                            item_id,
                            output_index,
                            sequence_number: next(sequence),
                            rest: Default::default(),
                        }),
                    ),
                ),
            ]
        }
        _ => Vec::new(),
    }
}
