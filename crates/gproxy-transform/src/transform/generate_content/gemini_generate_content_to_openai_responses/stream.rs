use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<openai::ResponseStreamEvent, TransformError> {
    let mut transform = StreamTransform;
    let mut output = transform.push(input, ctx)?;
    Ok(output
        .drain(..)
        .next()
        .unwrap_or_else(default_response_in_progress))
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
    let usage = usage_metadata.map(common::gemini_usage_to_completion);
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
                Some(openai::IncompleteDetails {
                    reason: Some(openai::IncompleteReason::ContentFilter),
                    extra: Default::default(),
                }),
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
        .filter_map(|part| part_to_response_event(part, output_index))
        .collect()
}

fn part_to_response_event(
    part: gemini::Part,
    output_index: u32,
) -> Option<openai::ResponseStreamEvent> {
    match part.data? {
        gemini::PartData::Text { text } => {
            if part.thought.unwrap_or(false) {
                Some(known(
                    openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                        content_index: 0,
                        delta: text,
                        item_id: reasoning_id(output_index),
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                ))
            } else {
                Some(known(
                    openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
                        content_index: 0,
                        delta: text,
                        item_id: message_id(output_index),
                        logprobs: None,
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                ))
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
            Some(known(
                openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                    item: Box::new(openai::ResponseOutputItem(openai::ResponseItem::Typed(
                        openai::TypedResponseItem::FunctionCall {
                            arguments: function_call
                                .args
                                .map(|args| serde_json::to_string(&args).unwrap_or_default())
                                .unwrap_or_default(),
                            call_id: call_id.clone(),
                            name: function_call.name,
                            id: Some(item_id),
                            namespace: None,
                            status: Some(openai::ResponseItemLifecycleStatus::Completed),
                            extra: Default::default(),
                        },
                    ))),
                    output_index,
                    sequence_number: None,
                    extra: Default::default(),
                },
            ))
        }
        _ => None,
    }
}

fn response_lifecycle_event(
    id: String,
    model: openai::OpenAiModelId,
    usage: Option<openai::CompletionUsage>,
    service_tier: Option<openai::ServiceTier>,
    status: openai::ResponseStatus,
    incomplete_details: Option<openai::IncompleteDetails>,
) -> openai::ResponseStreamEvent {
    let event_status = status.clone();
    let response = Box::new(openai::ResponseObject {
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
        usage: common::completion_usage_to_response(usage),
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

fn response_status_from_gemini_finish(
    reason: gemini::FinishReason,
) -> (openai::ResponseStatus, Option<openai::IncompleteDetails>) {
    match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => (
            openai::ResponseStatus::Incomplete,
            Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            }),
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
            Some(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                extra: Default::default(),
            }),
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

fn default_response_in_progress() -> openai::ResponseStreamEvent {
    response_lifecycle_event(
        String::new(),
        common::default_openai_model(),
        None,
        None,
        openai::ResponseStatus::InProgress,
        None,
    )
}
