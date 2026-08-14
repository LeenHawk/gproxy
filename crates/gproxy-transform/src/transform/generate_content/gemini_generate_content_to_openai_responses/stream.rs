use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::gemini_usage_to_response;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
    let mut transform = StreamTransform;
    transform.push(input, ctx)
}

#[derive(Default)]
pub struct StreamTransform;

impl StreamTransform {
    pub fn push(
        &mut self,
        input: gemini::StreamGenerateContentChunk,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(gemini_chunk_to_response_events(input))
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(Vec::new())
    }
}

fn gemini_chunk_to_response_events(
    input: gemini::GenerateContentResponse,
) -> Vec<openai::ResponseStreamEvent> {
    let id = input.response_id.unwrap_or_default();
    let model = input
        .model_version
        .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
        .into();
    let usage_metadata = input.usage_metadata;
    let service_tier = usage_metadata
        .as_ref()
        .and_then(|usage| common::gemini_service_tier_to_openai(usage.service_tier.clone()));
    let usage = usage_metadata.map(gemini_usage_to_response);
    let blocked = input
        .prompt_feedback
        .as_ref()
        .and_then(|feedback| feedback.block_reason.as_ref())
        .is_some();

    if input.candidates.is_empty() {
        return vec![if blocked {
            response_lifecycle_event(
                id,
                model,
                usage,
                service_tier,
                openai::ResponseStatus::Incomplete,
                Some(crate::protocol::wire!(openai::IncompleteDetails {
                    reason: Some(openai::IncompleteReason::ContentFilter),
                    extra: Default::default(),
                })),
            )
        } else {
            response_lifecycle_event(
                id,
                model,
                usage,
                service_tier,
                openai::ResponseStatus::InProgress,
                None,
            )
        }];
    }

    let mut output = Vec::new();
    for (fallback_index, candidate) in input.candidates.into_iter().enumerate() {
        let output_index = candidate
            .index
            .map(|index| u32::try_from(index).unwrap_or_default())
            .unwrap_or_else(|| u32::try_from(fallback_index).unwrap_or_default());

        if let Some(content) = candidate.content {
            output.extend(gemini_content_to_response_events(content, output_index));
        }

        if let Some(finish_reason) = candidate.finish_reason {
            let (status, incomplete_details) = response_status_from_gemini_finish(finish_reason);
            output.push(response_lifecycle_event(
                id.clone(),
                model.clone(),
                usage.clone(),
                service_tier.clone(),
                status,
                incomplete_details,
            ));
        }
    }

    if output.is_empty() {
        output.push(response_lifecycle_event(
            id,
            model,
            usage,
            service_tier,
            openai::ResponseStatus::InProgress,
            None,
        ));
    }

    output
}

fn gemini_content_to_response_events(
    content: gemini::Content,
    output_index: u32,
) -> Vec<openai::ResponseStreamEvent> {
    content
        .parts
        .into_iter()
        .flat_map(|part| part_to_response_events(part, output_index))
        .collect()
}

fn part_to_response_events(
    part: gemini::Part,
    output_index: u32,
) -> Vec<openai::ResponseStreamEvent> {
    let signature = part.thought_signature.clone();
    let Some(data) = part.data else {
        return signature
            .map(|signature| vec![reasoning_done(output_index, None, signature)])
            .unwrap_or_default();
    };
    match data {
        gemini::PartData::Text { text } => {
            if part.thought.unwrap_or(false) {
                let mut events = vec![known(
                    openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                        content_index: 0,
                        delta: text.clone(),
                        item_id: reasoning_id(output_index),
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                )];
                if let Some(signature) = signature {
                    events.push(reasoning_done(output_index, Some(text), signature));
                }
                events
            } else {
                vec![known(
                    openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                        content_index: 0,
                        delta: text,
                        item_id: message_id(output_index),
                        logprobs: None,
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                )]
            }
        }
        gemini::PartData::FunctionCall { function_call } => {
            let (call_id, item_id) = function_call.id.as_deref().map_or_else(
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
            );
            let mut events = Vec::new();
            if let Some(signature) = signature {
                events.push(reasoning_done(output_index, None, signature));
            }
            events.push(known(
                openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                    item: Box::new(openai::ResponseOutputItem::new(
                        openai::ResponseItem::Typed(crate::protocol::wire!(
                            openai::TypedResponseItem::FunctionCall {
                                arguments: function_call
                                    .args
                                    .map(|args| serde_json::to_string(&args).unwrap_or_default())
                                    .unwrap_or_default(),
                                call_id: call_id.clone(),
                                name: function_call.name,
                                id: Some(item_id),
                                caller: None,
                                namespace: None,
                                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                                extra: Default::default(),
                            }
                        )),
                    )),
                    output_index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ));
            events
        }
        _ => Vec::new(),
    }
}

fn reasoning_done(
    output_index: u32,
    text: Option<String>,
    encrypted_content: String,
) -> openai::ResponseStreamEvent {
    known(openai::KnownResponseStreamEvent::ResponseOutputItemDone {
        item: Box::new(openai::ResponseOutputItem::new(
            openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
                id: Some(reasoning_id(output_index)),
                summary: Vec::new(),
                content: text.map(|text| {
                    vec![crate::protocol::wire!(openai::ResponseReasoningTextPart {
                        text,
                        type_: openai::ResponseReasoningTextType::ReasoningText,
                        extra: Default::default(),
                    })]
                }),
                encrypted_content: Some(encrypted_content),
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                extra: Default::default(),
            }),
        )),
        output_index,
        sequence_number: None,
        extra: Default::default(),
    })
}

fn response_lifecycle_event(
    id: String,
    model: openai::OpenAiModelId,
    usage: Option<openai::ResponseUsage>,
    service_tier: Option<openai::ServiceTier>,
    status: openai::ResponseStatus,
    incomplete_details: Option<openai::IncompleteDetails>,
) -> openai::ResponseStreamEvent {
    let event_status = status.clone();
    let response = Box::new(crate::protocol::wire!(openai::ResponseObject {
        id,
        created_at: 0,
        background: None,
        completed_at: matches!(status, openai::ResponseStatus::Completed).then_some(0),
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(model),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output: Vec::new(),
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier,
        status: Some(status),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage,
        user: None,
        extra: Default::default(),
    }));

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

fn response_status_from_gemini_finish(
    reason: gemini::FinishReason,
) -> (openai::ResponseStatus, Option<openai::IncompleteDetails>) {
    match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            })),
        ),
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                extra: Default::default(),
            })),
        ),
        _ => (openai::ResponseStatus::Completed, None),
    }
}

fn message_id(index: u32) -> String {
    format!("msg_{index}")
}

fn reasoning_id(index: u32) -> String {
    format!("reasoning_{index}")
}

fn known(event: openai::KnownResponseStreamEvent) -> openai::ResponseStreamEvent {
    openai::ResponseStreamEvent::Known(event)
}
