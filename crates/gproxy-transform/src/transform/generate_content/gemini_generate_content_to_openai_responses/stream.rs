use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::gemini_usage_to_response;

pub fn stream_event(
    input: gemini::StreamGenerateContentChunk,
    ctx: &TransformContext,
) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
    let mut transform = StreamTransform::default();
    transform.push(input, ctx)
}

#[derive(Default)]
pub struct StreamTransform {
    started: bool,
    next_output_index: u32,
    message_index: Option<u32>,
    reasoning_index: Option<u32>,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: gemini::StreamGenerateContentChunk,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let mut output = Vec::new();
        if !self.started {
            output.push(response_created_event(&input));
            self.started = true;
        }
        output.extend(self.gemini_chunk_to_response_events(input));
        Ok(output)
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(Vec::new())
    }

    fn gemini_chunk_to_response_events(
        &mut self,
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
        for candidate in input.candidates {
            if let Some(content) = candidate.content {
                for part in content.parts {
                    output.extend(self.part_to_response_events(part));
                }
            }

            if let Some(finish_reason) = candidate.finish_reason {
                let (status, incomplete_details) =
                    response_status_from_gemini_finish(finish_reason);
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

    fn part_to_response_events(&mut self, part: gemini::Part) -> Vec<openai::ResponseStreamEvent> {
        let signature = part.thought_signature.clone();
        let Some(data) = part.data else {
            return signature
                .map(|signature| {
                    let output_index = self.reasoning_output_index();
                    vec![reasoning_done(output_index, None, signature)]
                })
                .unwrap_or_default();
        };
        match data {
            gemini::PartData::Text { text } if part.thought.unwrap_or(false) => {
                let output_index = self.reasoning_output_index();
                let mut events = Vec::new();
                if !text.is_empty() {
                    events.push(known(
                        openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
                            content_index: 0,
                            delta: text.clone(),
                            item_id: reasoning_id(output_index),
                            output_index,
                            sequence_number: None,
                            extra: Default::default(),
                        },
                    ));
                }
                if let Some(signature) = signature {
                    events.push(reasoning_done(output_index, Some(text), signature));
                }
                events
            }
            gemini::PartData::Text { text } => {
                if text.is_empty() {
                    return Vec::new();
                }
                let output_index = self.message_output_index();
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
            gemini::PartData::FunctionCall { function_call } => {
                let output_index = self.allocate_output_index();
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
                let mut extra = openai::Extra::new();
                if let Some(signature) = signature {
                    extra.insert(
                        "thought_signature".to_owned(),
                        serde_json::Value::String(signature),
                    );
                }
                vec![known(
                    openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
                        item: Box::new(openai::ResponseOutputItem::new(
                            openai::ResponseItem::Typed(crate::protocol::wire!(
                                openai::TypedResponseItem::FunctionCall {
                                    arguments: function_call
                                        .args
                                        .map(|args| {
                                            serde_json::to_string(&args).unwrap_or_default()
                                        })
                                        .unwrap_or_default(),
                                    call_id: call_id.clone(),
                                    name: function_call.name,
                                    id: Some(item_id),
                                    caller: None,
                                    namespace: None,
                                    status: Some(openai::ResponseItemLifecycleStatus::Completed,),
                                    extra,
                                }
                            )),
                        )),
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                )]
            }
            _ => Vec::new(),
        }
    }

    fn allocate_output_index(&mut self) -> u32 {
        let index = self.next_output_index;
        self.next_output_index = self.next_output_index.saturating_add(1);
        index
    }

    fn message_output_index(&mut self) -> u32 {
        if let Some(index) = self.message_index {
            index
        } else {
            let index = self.allocate_output_index();
            self.message_index = Some(index);
            index
        }
    }

    fn reasoning_output_index(&mut self) -> u32 {
        if let Some(index) = self.reasoning_index {
            index
        } else {
            let index = self.allocate_output_index();
            self.reasoning_index = Some(index);
            index
        }
    }
}

fn response_created_event(input: &gemini::GenerateContentResponse) -> openai::ResponseStreamEvent {
    let model = input
        .model_version
        .clone()
        .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
        .into();
    let service_tier = input
        .usage_metadata
        .as_ref()
        .and_then(|usage| common::gemini_service_tier_to_openai(usage.service_tier.clone()));
    let mut event = response_lifecycle_event(
        input.response_id.clone().unwrap_or_default(),
        model,
        None,
        service_tier,
        openai::ResponseStatus::InProgress,
        None,
    );
    let openai::ResponseStreamEvent::Known(openai::KnownResponseStreamEvent::ResponseInProgress {
        response,
        sequence_number,
        extra,
    }) = event
    else {
        unreachable!()
    };
    event = known(openai::KnownResponseStreamEvent::ResponseCreated {
        response,
        sequence_number,
        extra,
    });
    event
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
    format!("rs_{index}")
}

fn known(event: openai::KnownResponseStreamEvent) -> openai::ResponseStreamEvent {
    openai::ResponseStreamEvent::Known(event)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        )
    }

    #[test]
    fn emits_created_and_distinct_output_indexes() {
        let first = serde_json::from_value(json!({
            "responseId": "r1",
            "modelVersion": "gemini-flash",
            "candidates": [{
                "index": 0,
                "content": {"role": "model", "parts": [
                    {
                        "functionCall": {
                            "id": "call_1",
                            "name": "get_magic",
                            "args": {"value": "x"}
                        },
                        "thoughtSignature": "signature"
                    },
                    {"text": "done"},
                    {"text": ""}
                ]}
            }]
        }))
        .unwrap();
        let last = serde_json::from_value(json!({
            "responseId": "r1",
            "modelVersion": "gemini-flash",
            "candidates": [{"index": 0, "finishReason": "STOP"}]
        }))
        .unwrap();
        let mut transform = StreamTransform::default();
        let mut events = transform.push(first, &ctx()).unwrap();
        events.extend(transform.push(last, &ctx()).unwrap());
        let values = events
            .into_iter()
            .map(|event| serde_json::to_value(event).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(values[0]["type"], "response.created");
        let tool = values
            .iter()
            .find(|event| event["type"] == "response.output_item.added")
            .unwrap();
        let text = values
            .iter()
            .find(|event| event["type"] == "response.output_text.delta")
            .unwrap();
        assert_eq!(tool["output_index"], 0);
        assert_eq!(text["output_index"], 1);
        assert_eq!(
            values.last().unwrap()["type"],
            serde_json::Value::String("response.completed".to_owned())
        );
        assert_eq!(
            values
                .iter()
                .filter(|event| event["type"] == "response.output_text.delta")
                .count(),
            1,
            "empty Gemini text parts must not create phantom Responses messages"
        );
    }
}
