use std::collections::BTreeMap;

use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::chat_usage_to_response;

pub fn stream_event(
    input: openai::ChatCompletionChunk,
    ctx: &TransformContext,
) -> Result<openai::ResponseStreamEvent, TransformError> {
    let mut transform = StreamTransform::default();
    let mut output = transform.push(input, ctx)?;
    Ok(output
        .drain(..)
        .next()
        .unwrap_or_else(default_response_in_progress))
}

#[derive(Default)]
pub struct StreamTransform {
    tool_calls: BTreeMap<u32, ResponseToolState>,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: openai::ChatCompletionChunk,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(self.chat_chunk_to_response_events(input))
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(Vec::new())
    }

    fn chat_chunk_to_response_events(
        &mut self,
        input: openai::ChatCompletionChunk,
    ) -> Vec<openai::ResponseStreamEvent> {
        let id = input.id;
        let created = input.created;
        let model = input.model;
        let usage = input.usage;

        if input.choices.is_empty() {
            return vec![response_lifecycle_event(
                id,
                created,
                model,
                usage,
                openai::ResponseStatus::Completed,
                None,
            )];
        }

        let mut output = Vec::new();
        let mut finish_reason = None;
        for choice in input.choices {
            let (events, reason) = self.choice_to_response_events(choice);
            output.extend(events);
            finish_reason = finish_reason.or(reason);
        }

        if let Some(reason) = finish_reason {
            let (status, incomplete) = response_status_from_finish(reason);
            output.push(response_lifecycle_event(
                id, created, model, usage, status, incomplete,
            ));
        } else if output.is_empty() {
            output.push(response_lifecycle_event(
                id,
                created,
                model,
                usage,
                openai::ResponseStatus::InProgress,
                None,
            ));
        }

        output
    }

    fn choice_to_response_events(
        &mut self,
        choice: openai::ChatChunkChoice,
    ) -> (
        Vec<openai::ResponseStreamEvent>,
        Option<openai::ChatFinishReason>,
    ) {
        let index = choice.index;
        let finish_reason = choice.finish_reason;
        let delta = choice.delta;
        let mut output = Vec::new();

        if let Some(content) = delta.content.filter(|value| !value.is_empty()) {
            output.push(known(
                openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                    content_index: 0,
                    delta: content,
                    item_id: message_item_id(index),
                    logprobs: None,
                    output_index: index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ));
        }

        if let Some(reasoning) = delta.reasoning_content.filter(|value| !value.is_empty()) {
            output.push(known(
                openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                    content_index: 0,
                    delta: reasoning,
                    item_id: reasoning_item_id(index),
                    output_index: index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ));
        }

        if let Some(refusal) = delta.refusal.filter(|value| !value.is_empty()) {
            output.push(known(
                openai::KnownResponseStreamEvent::ResponseRefusalDelta {
                    content_index: 0,
                    delta: refusal,
                    item_id: message_item_id(index),
                    output_index: index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ));
        }

        if let Some(tool_calls) = delta.tool_calls {
            for call in tool_calls {
                output.extend(self.tool_delta_to_response_events(call));
            }
        }

        if let Some(function_call) = delta.function_call {
            output.extend(self.legacy_function_delta_to_response_events(index, function_call));
        }

        (output, finish_reason)
    }

    fn legacy_function_delta_to_response_events(
        &mut self,
        output_index: u32,
        function_call: openai::FunctionCallDelta,
    ) -> Vec<openai::ResponseStreamEvent> {
        let state = self.function_state(None, output_index);
        let mut output = Vec::new();

        if let Some(name) = function_call.name.filter(|value| !value.is_empty()) {
            output.push(output_item_added(
                output_index,
                openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                    arguments: String::new(),
                    call_id: state.call_id.clone(),
                    name,
                    id: Some(state.item_id.clone()),
                    namespace: None,
                    status: Some(openai::ResponseItemLifecycleStatus::InProgress),
                    extra: Default::default(),
                }),
            ));
        }

        if let Some(arguments) = function_call.arguments.filter(|value| !value.is_empty()) {
            output.push(known(
                openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                    delta: arguments,
                    item_id: state.item_id,
                    output_index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ));
        }

        output
    }

    fn tool_delta_to_response_events(
        &mut self,
        call: openai::ChatToolCallDelta,
    ) -> Vec<openai::ResponseStreamEvent> {
        let output_index = call.index;
        if let Some(function) = call.function {
            let state = self.function_state(call.id.as_deref(), output_index);
            let mut output = Vec::new();

            if let Some(name) = function.name.filter(|value| !value.is_empty()) {
                output.push(output_item_added(
                    output_index,
                    openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                        arguments: String::new(),
                        call_id: state.call_id.clone(),
                        name,
                        id: Some(state.item_id.clone()),
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::InProgress),
                        extra: Default::default(),
                    }),
                ));
            }

            if let Some(arguments) = function.arguments.filter(|value| !value.is_empty()) {
                output.push(known(
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                        delta: arguments,
                        item_id: state.item_id,
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                ));
            }

            return output;
        }

        if let Some(custom) = call.custom {
            let state = self.custom_state(call.id.as_deref(), output_index);
            let mut output = Vec::new();

            if let Some(name) = custom.name.filter(|value| !value.is_empty()) {
                output.push(output_item_added(
                    output_index,
                    openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                        call_id: state.call_id.clone(),
                        input: String::new(),
                        name,
                        id: None,
                        namespace: None,
                        extra: Default::default(),
                    }),
                ));
            }

            if let Some(input) = custom.input.filter(|value| !value.is_empty()) {
                output.push(known(
                    openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
                        delta: input,
                        item_id: state.item_id,
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                ));
            }

            return output;
        }

        Vec::new()
    }

    fn function_state(
        &mut self,
        chat_call_id: Option<&str>,
        output_index: u32,
    ) -> ResponseToolState {
        if let Some(state) = self.tool_calls.get(&output_index) {
            return state.clone();
        }
        let (call_id, item_id) = response_function_ids(chat_call_id, output_index);
        let state = ResponseToolState { call_id, item_id };
        self.tool_calls.insert(output_index, state.clone());
        state
    }

    fn custom_state(&mut self, chat_call_id: Option<&str>, output_index: u32) -> ResponseToolState {
        if let Some(state) = self.tool_calls.get(&output_index) {
            return state.clone();
        }
        let call_id = chat_call_id
            .map(common::response_call_id)
            .unwrap_or_else(|| common::indexed_response_call_id(output_index));
        let state = ResponseToolState {
            call_id,
            item_id: format!("ctc_{output_index}"),
        };
        self.tool_calls.insert(output_index, state.clone());
        state
    }
}

#[derive(Clone)]
struct ResponseToolState {
    call_id: String,
    item_id: String,
}

fn output_item_added(output_index: u32, item: openai::ResponseItem) -> openai::ResponseStreamEvent {
    known(openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
        item: Box::new(openai::ResponseOutputItem(item)),
        output_index,
        sequence_number: None,
        extra: Default::default(),
    })
}

fn response_lifecycle_event(
    id: String,
    created_at: u64,
    model: openai::OpenAiModelId,
    usage: Option<openai::CompletionUsage>,
    status: openai::ResponseStatus,
    incomplete_details: Option<openai::IncompleteDetails>,
) -> openai::ResponseStreamEvent {
    let event_status = status.clone();
    let response = Box::new(openai::ResponseObject {
        id,
        created_at,
        background: None,
        completed_at: matches!(status, openai::ResponseStatus::Completed).then_some(created_at),
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(model),
        moderation: None,
        object: openai::ResponseObjectType::Response,
        output: Vec::new(),
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        status: Some(status),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: chat_usage_to_response(usage),
        user: None,
        extra: Default::default(),
    });

    match event_status {
        openai::ResponseStatus::Completed => {
            known(openai::KnownResponseStreamEvent::ResponseCompleted {
                response,
                sequence_number: None,
                extra: Default::default(),
            })
        }
        openai::ResponseStatus::Incomplete => {
            known(openai::KnownResponseStreamEvent::ResponseIncomplete {
                response,
                sequence_number: None,
                extra: Default::default(),
            })
        }
        _ => known(openai::KnownResponseStreamEvent::ResponseInProgress {
            response,
            sequence_number: None,
            extra: Default::default(),
        }),
    }
}

fn response_status_from_finish(
    reason: openai::ChatFinishReason,
) -> (openai::ResponseStatus, Option<openai::IncompleteDetails>) {
    match reason {
        openai::ChatFinishReason::Length => (
            openai::ResponseStatus::Incomplete,
            Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            }),
        ),
        openai::ChatFinishReason::ContentFilter => (
            openai::ResponseStatus::Incomplete,
            Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                extra: Default::default(),
            }),
        ),
        _ => (openai::ResponseStatus::Completed, None),
    }
}

fn message_item_id(index: u32) -> String {
    format!("msg_{index}")
}

fn reasoning_item_id(index: u32) -> String {
    format!("reasoning_{index}")
}

fn response_function_ids(chat_call_id: Option<&str>, output_index: u32) -> (String, String) {
    chat_call_id.map_or_else(
        || {
            (
                common::indexed_response_call_id(output_index),
                common::indexed_response_function_call_item_id(output_index),
            )
        },
        |id| {
            (
                common::response_call_id(id),
                common::response_function_call_item_id(id),
            )
        },
    )
}

fn known(event: openai::KnownResponseStreamEvent) -> openai::ResponseStreamEvent {
    openai::ResponseStreamEvent::Known(event)
}

fn default_response_in_progress() -> openai::ResponseStreamEvent {
    response_lifecycle_event(
        String::new(),
        0,
        common::default_openai_model(),
        None,
        openai::ResponseStatus::InProgress,
        None,
    )
}
