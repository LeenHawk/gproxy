use std::collections::BTreeMap;

use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::response_usage_to_chat;

pub fn stream_event(
    input: openai::ResponseStreamEvent,
    ctx: &TransformContext,
) -> Result<openai::ChatCompletionChunk, TransformError> {
    let mut transform = StreamTransform::default();
    let mut output = transform.push(input, ctx)?;
    Ok(output.drain(..).next().unwrap_or_else(empty_unknown_chunk))
}

#[derive(Default)]
pub struct StreamTransform {
    tool_calls: BTreeMap<String, ToolCallState>,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: openai::ResponseStreamEvent,
        _: &TransformContext,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        Ok(match input {
            openai::ResponseStreamEvent::Known(event) => self.known_event_to_chat(event),
            openai::ResponseStreamEvent::Unknown(_) => Vec::new(),
        })
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        Ok(Vec::new())
    }

    fn known_event_to_chat(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Vec<openai::ChatCompletionChunk> {
        match event {
            openai::KnownResponseStreamEvent::ResponseCreated { response, .. }
            | openai::KnownResponseStreamEvent::ResponseInProgress { response, .. }
            | openai::KnownResponseStreamEvent::ResponseQueued { response, .. } => {
                vec![empty_from_response(*response)]
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                item,
                output_index,
                ..
            } => self.output_item_added_to_chat(*item, output_index),
            openai::KnownResponseStreamEvent::ResponseCompleted { response, .. }
            | openai::KnownResponseStreamEvent::ResponseFailed { response, .. }
            | openai::KnownResponseStreamEvent::ResponseIncomplete { response, .. } => {
                vec![finish_from_response(*response)]
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                delta,
                item_id,
                output_index: _,
                ..
            } => vec![common::chat_text_delta(
                item_id,
                common::default_openai_model(),
                0,
                0,
                delta,
            )],
            openai::KnownResponseStreamEvent::ResponseAudioTranscriptDelta { delta, .. } => {
                vec![common::chat_text_delta(
                    "resp_audio_transcript".to_owned(),
                    common::default_openai_model(),
                    0,
                    0,
                    delta,
                )]
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta {
                delta,
                item_id,
                output_index: _,
                ..
            } => vec![common::chat_refusal_delta(
                item_id,
                common::default_openai_model(),
                0,
                0,
                delta,
            )],
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta {
                delta,
                item_id,
                output_index: _,
                ..
            }
            | openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                delta,
                item_id,
                output_index: _,
                ..
            } => vec![common::chat_reasoning_delta(
                item_id,
                common::default_openai_model(),
                0,
                0,
                delta,
            )],
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let call_id = self.call_id_for_item(output_index, &item_id);
                vec![chat_tool_delta(
                    call_id.clone(),
                    output_index,
                    common::chat_function_tool_delta(
                        output_index,
                        Some(call_id),
                        None,
                        Some(delta),
                    ),
                    None,
                )]
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                output_index,
                ..
            } => self.function_call_done_to_chat(output_index, item_id, name, arguments),
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
                delta,
                item_id,
                output_index,
                ..
            } => {
                let call_id = self.call_id_for_item(output_index, &item_id);
                vec![chat_tool_delta(
                    call_id.clone(),
                    output_index,
                    common::chat_custom_tool_delta(output_index, Some(call_id), None, Some(delta)),
                    None,
                )]
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone {
                input,
                item_id,
                output_index,
                ..
            } => {
                let call_id = self.call_id_for_item(output_index, &item_id);
                non_empty(input)
                    .map(|input| {
                        chat_tool_delta(
                            call_id.clone(),
                            output_index,
                            common::chat_custom_tool_delta(
                                output_index,
                                Some(call_id),
                                None,
                                Some(input),
                            ),
                            None,
                        )
                    })
                    .into_iter()
                    .collect()
            }
            openai::KnownResponseStreamEvent::Error { .. } => vec![common::chat_finish_chunk(
                "resp_error".to_owned(),
                common::default_openai_model(),
                0,
                openai::ChatFinishReason::ContentFilter,
                None,
            )],
            _ => Vec::new(),
        }
    }

    fn output_item_added_to_chat(
        &mut self,
        item: openai::ResponseOutputItem,
        output_index: u32,
    ) -> Vec<openai::ChatCompletionChunk> {
        match item.0 {
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                id,
                ..
            }) => {
                if let Some(item_id) = id {
                    self.tool_calls.insert(
                        item_id,
                        ToolCallState {
                            call_id: call_id.clone(),
                            name: Some(name.clone()),
                        },
                    );
                }

                let mut chunks = vec![chat_tool_delta(
                    call_id.clone(),
                    output_index,
                    common::chat_function_tool_delta(
                        output_index,
                        Some(call_id.clone()),
                        Some(name),
                        None,
                    ),
                    None,
                )];
                if let Some(arguments) = non_empty(arguments) {
                    chunks.push(chat_tool_delta(
                        call_id.clone(),
                        output_index,
                        common::chat_function_tool_delta(
                            output_index,
                            Some(call_id.clone()),
                            None,
                            Some(arguments),
                        ),
                        None,
                    ));
                }
                chunks
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                call_id,
                input,
                name,
                id,
                ..
            }) => {
                if let Some(item_id) = id {
                    self.tool_calls.insert(
                        item_id,
                        ToolCallState {
                            call_id: call_id.clone(),
                            name: Some(name.clone()),
                        },
                    );
                }

                let mut chunks = vec![chat_tool_delta(
                    call_id.clone(),
                    output_index,
                    common::chat_custom_tool_delta(
                        output_index,
                        Some(call_id.clone()),
                        Some(name),
                        None,
                    ),
                    None,
                )];
                if let Some(input) = non_empty(input) {
                    chunks.push(chat_tool_delta(
                        call_id.clone(),
                        output_index,
                        common::chat_custom_tool_delta(
                            output_index,
                            Some(call_id.clone()),
                            None,
                            Some(input),
                        ),
                        None,
                    ));
                }
                chunks
            }
            _ => Vec::new(),
        }
    }

    fn function_call_done_to_chat(
        &mut self,
        output_index: u32,
        item_id: String,
        name: String,
        arguments: String,
    ) -> Vec<openai::ChatCompletionChunk> {
        let state = self.tool_calls.get(&item_id).cloned();
        let call_id = state
            .as_ref()
            .map(|state| state.call_id.clone())
            .unwrap_or_else(|| common::fallback_response_call_id(output_index, Some(&item_id)));
        let mut chunks = Vec::new();

        if state
            .as_ref()
            .and_then(|state| state.name.as_ref())
            .is_none()
        {
            chunks.push(chat_tool_delta(
                call_id.clone(),
                output_index,
                common::chat_function_tool_delta(
                    output_index,
                    Some(call_id.clone()),
                    Some(name),
                    None,
                ),
                None,
            ));
        }
        if let Some(arguments) = non_empty(arguments) {
            chunks.push(chat_tool_delta(
                call_id.clone(),
                output_index,
                common::chat_function_tool_delta(
                    output_index,
                    Some(call_id.clone()),
                    None,
                    Some(arguments),
                ),
                None,
            ));
        }
        chunks
    }

    fn call_id_for_item(&self, output_index: u32, item_id: &str) -> String {
        self.tool_calls
            .get(item_id)
            .map(|state| state.call_id.clone())
            .unwrap_or_else(|| common::fallback_response_call_id(output_index, Some(item_id)))
    }
}

#[derive(Clone)]
struct ToolCallState {
    call_id: String,
    name: Option<String>,
}

fn chat_tool_delta(
    id: String,
    _index: u32,
    tool_delta: openai::ChatToolCallDelta,
    finish_reason: Option<openai::ChatFinishReason>,
) -> openai::ChatCompletionChunk {
    let mut delta = common::empty_chat_delta();
    delta.tool_calls = Some(vec![tool_delta]);
    common::chat_delta_chunk(
        id,
        common::default_openai_model(),
        0,
        0,
        delta,
        finish_reason,
        None,
    )
}

fn empty_from_response(response: openai::ResponseObject) -> openai::ChatCompletionChunk {
    let id = response.id;
    let created = response.created_at;
    let model = response.model.unwrap_or_else(common::default_openai_model);
    let usage = response_usage_to_chat(response.usage);
    common::empty_chat_chunk(id, model, created, usage)
}

fn finish_from_response(response: openai::ResponseObject) -> openai::ChatCompletionChunk {
    let finish_reason = response_finish_reason(&response);
    let id = response.id;
    let created = response.created_at;
    let model = response.model.unwrap_or_else(common::default_openai_model);
    let usage = response_usage_to_chat(response.usage);
    common::chat_finish_chunk(id, model, created, finish_reason, usage)
}

fn response_finish_reason(response: &openai::ResponseObject) -> openai::ChatFinishReason {
    if response.output.iter().any(|item| {
        matches!(
            &item.0,
            openai::ResponseItem::Typed(
                openai::TypedResponseItem::FunctionCall { .. }
                    | openai::TypedResponseItem::CustomToolCall { .. }
            )
        )
    }) {
        return openai::ChatFinishReason::ToolCalls;
    }

    match response.status {
        Some(openai::ResponseStatus::Incomplete) => response
            .incomplete_details
            .as_ref()
            .and_then(|details| details.reason.as_ref())
            .map(incomplete_reason_to_chat)
            .unwrap_or(openai::ChatFinishReason::Length),
        Some(openai::ResponseStatus::Failed | openai::ResponseStatus::Cancelled) => {
            openai::ChatFinishReason::ContentFilter
        }
        _ => openai::ChatFinishReason::Stop,
    }
}

fn incomplete_reason_to_chat(reason: &openai::IncompleteReason) -> openai::ChatFinishReason {
    match reason {
        openai::IncompleteReason::MaxOutputTokens => openai::ChatFinishReason::Length,
        openai::IncompleteReason::ContentFilter => openai::ChatFinishReason::ContentFilter,
    }
}

fn empty_unknown_chunk() -> openai::ChatCompletionChunk {
    common::empty_chat_chunk(
        "resp_unknown".to_owned(),
        common::default_openai_model(),
        0,
        None,
    )
}

fn non_empty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
