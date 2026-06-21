use std::collections::BTreeMap;

use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: openai::ResponseStreamEvent,
    ctx: &TransformContext,
) -> Result<gemini::StreamGenerateContentChunk, TransformError> {
    let mut transform = StreamTransform::default();
    let mut output = transform.push(input, ctx)?;
    Ok(output.drain(..).next().unwrap_or_else(empty_chunk))
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
    ) -> Result<Vec<gemini::StreamGenerateContentChunk>, TransformError> {
        Ok(match input {
            openai::ResponseStreamEvent::Known(event) => self.known_event_to_gemini(event),
            openai::ResponseStreamEvent::Unknown(_) => Vec::new(),
        })
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<gemini::StreamGenerateContentChunk>, TransformError> {
        Ok(Vec::new())
    }

    fn known_event_to_gemini(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Vec<gemini::GenerateContentResponse> {
        match event {
            openai::KnownResponseStreamEvent::ResponseCreated { response, .. }
            | openai::KnownResponseStreamEvent::ResponseInProgress { response, .. }
            | openai::KnownResponseStreamEvent::ResponseQueued { response, .. } => {
                vec![chunk_from_response(*response, None)]
            }
            openai::KnownResponseStreamEvent::ResponseCompleted { response, .. }
            | openai::KnownResponseStreamEvent::ResponseFailed { response, .. }
            | openai::KnownResponseStreamEvent::ResponseIncomplete { response, .. } => {
                let finish_reason = Some(response_finish_reason(&response));
                vec![chunk_from_response(*response, finish_reason)]
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta { delta, .. }
            | openai::KnownResponseStreamEvent::ResponseAudioTranscriptDelta { delta, .. } => {
                vec![text_chunk(delta, false, None)]
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta { delta, .. } => {
                vec![text_chunk(
                    delta,
                    false,
                    Some(gemini::FinishReasonKnown::Safety),
                )]
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta {
                delta, ..
            }
            | openai::KnownResponseStreamEvent::ResponseReasoningTextDelta { delta, .. } => {
                vec![text_chunk(delta, true, None)]
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded { item, .. } => {
                self.response_item_to_gemini(*item).into_iter().collect()
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                delta,
                item_id,
                ..
            }
            | openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta {
                delta,
                item_id,
                ..
            } => self
                .tool_calls
                .get(&item_id)
                .map(|state| {
                    function_call_chunk(
                        Some(state.call_id.clone()),
                        state.name.clone(),
                        arguments_to_json_map(delta),
                    )
                })
                .into_iter()
                .collect(),
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone {
                arguments,
                item_id,
                name,
                output_index,
                ..
            } => {
                let state =
                    self.tool_calls
                        .get(&item_id)
                        .cloned()
                        .unwrap_or_else(|| ToolCallState {
                            call_id: common::fallback_response_call_id(
                                output_index,
                                Some(&item_id),
                            ),
                            name,
                        });
                vec![function_call_chunk(
                    Some(state.call_id),
                    state.name,
                    arguments_to_json_map(arguments),
                )]
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone {
                input,
                item_id,
                ..
            } => self
                .tool_calls
                .get(&item_id)
                .map(|state| {
                    function_call_chunk(
                        Some(state.call_id.clone()),
                        state.name.clone(),
                        arguments_to_json_map(input),
                    )
                })
                .into_iter()
                .collect(),
            openai::KnownResponseStreamEvent::Error { .. } => vec![finish_chunk(
                gemini::FinishReason::Known(gemini::FinishReasonKnown::Safety),
                None,
            )],
            _ => Vec::new(),
        }
    }

    fn response_item_to_gemini(
        &mut self,
        item: openai::ResponseOutputItem,
    ) -> Option<gemini::GenerateContentResponse> {
        match item.0 {
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                id,
                ..
            }) => {
                if let Some(item_id) = id {
                    self.tool_calls.insert(
                        item_id,
                        ToolCallState {
                            call_id: call_id.clone(),
                            name: name.clone(),
                        },
                    );
                }
                Some(function_call_chunk(
                    Some(call_id),
                    name,
                    arguments_to_json_map(arguments),
                ))
            }
            openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
                call_id,
                name,
                input,
                id,
                ..
            }) => {
                if let Some(item_id) = id {
                    self.tool_calls.insert(
                        item_id,
                        ToolCallState {
                            call_id: call_id.clone(),
                            name: name.clone(),
                        },
                    );
                }
                Some(function_call_chunk(
                    Some(call_id),
                    name,
                    arguments_to_json_map(input),
                ))
            }
            _ => None,
        }
    }
}

#[derive(Clone)]
struct ToolCallState {
    call_id: String,
    name: String,
}

fn text_chunk(
    text: String,
    thought: bool,
    finish_reason: Option<gemini::FinishReasonKnown>,
) -> gemini::GenerateContentResponse {
    candidate_chunk(
        Some(gemini::Content {
            parts: vec![gemini::Part {
                thought: thought.then_some(true),
                data: Some(gemini::PartData::Text { text }),
                ..Default::default()
            }],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }),
        finish_reason.map(gemini::FinishReason::Known),
        None,
    )
}

fn function_call_chunk(
    id: Option<String>,
    name: String,
    args: Option<gemini::JsonMap>,
) -> gemini::GenerateContentResponse {
    candidate_chunk(
        Some(gemini::Content {
            parts: vec![gemini::Part {
                data: Some(gemini::PartData::FunctionCall {
                    function_call: gemini::FunctionCall {
                        id,
                        name,
                        args,
                        extra: Default::default(),
                    },
                }),
                ..Default::default()
            }],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }),
        None,
        None,
    )
}

fn finish_chunk(
    finish_reason: gemini::FinishReason,
    usage: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    candidate_chunk(None, Some(finish_reason), usage)
}

fn candidate_chunk(
    content: Option<gemini::Content>,
    finish_reason: Option<gemini::FinishReason>,
    usage_metadata: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    gemini::GenerateContentResponse {
        candidates: vec![gemini::Candidate {
            content,
            finish_reason,
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
            extra: Default::default(),
        }],
        prompt_feedback: None,
        usage_metadata,
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

fn chunk_from_response(
    response: openai::ResponseObject,
    finish_reason: Option<gemini::FinishReason>,
) -> gemini::GenerateContentResponse {
    let usage_metadata =
        common::completion_usage_to_gemini(common::response_usage_to_completion(response.usage));
    let mut chunk = if let Some(finish_reason) = finish_reason {
        finish_chunk(finish_reason, usage_metadata)
    } else {
        empty_chunk_with_usage(usage_metadata)
    };
    chunk.model_version = response.model.map(common::openai_model_string);
    chunk.response_id = Some(response.id);
    chunk
}

fn empty_chunk_with_usage(
    usage_metadata: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    gemini::GenerateContentResponse {
        candidates: Vec::new(),
        prompt_feedback: None,
        usage_metadata,
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

fn empty_chunk() -> gemini::GenerateContentResponse {
    empty_chunk_with_usage(None)
}

fn response_finish_reason(response: &openai::ResponseObject) -> gemini::FinishReason {
    match response.status {
        Some(openai::ResponseStatus::Incomplete) => response
            .incomplete_details
            .as_ref()
            .and_then(|details| details.reason.as_ref())
            .map(incomplete_reason_to_gemini)
            .unwrap_or(gemini::FinishReason::Known(
                gemini::FinishReasonKnown::MaxTokens,
            )),
        Some(openai::ResponseStatus::Failed | openai::ResponseStatus::Cancelled) => {
            gemini::FinishReason::Known(gemini::FinishReasonKnown::Safety)
        }
        _ => gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop),
    }
}

fn incomplete_reason_to_gemini(reason: &openai::IncompleteReason) -> gemini::FinishReason {
    match reason {
        openai::IncompleteReason::MaxOutputTokens => {
            gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens)
        }
        openai::IncompleteReason::ContentFilter => {
            gemini::FinishReason::Known(gemini::FinishReasonKnown::Safety)
        }
    }
}

fn arguments_to_json_map(value: String) -> Option<gemini::JsonMap> {
    serde_json::from_str(&value).ok()
}
