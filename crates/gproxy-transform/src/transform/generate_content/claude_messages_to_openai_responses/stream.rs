use std::collections::BTreeMap;

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: claude::StreamEvent,
    ctx: &TransformContext,
) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
    let mut transform = StreamTransform::default();
    transform.push(input, ctx)
}

#[derive(Default)]
pub struct StreamTransform {
    item_ids: BTreeMap<u32, String>,
    reasoning_text: BTreeMap<u32, String>,
    service_tier: Option<openai::ServiceTier>,
}

impl StreamTransform {
    pub fn push(
        &mut self,
        input: claude::StreamEvent,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(match input {
            claude::StreamEvent::Known(event) => self.known_event_to_response(*event),
            claude::StreamEvent::Unknown(_) => Vec::new(),
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        })
    }

    pub fn finish(
        &mut self,
        _: &TransformContext,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        Ok(Vec::new())
    }

    fn known_event_to_response(
        &mut self,
        event: claude::KnownStreamEvent,
    ) -> Vec<openai::ResponseStreamEvent> {
        match event {
            claude::KnownStreamEvent::MessageStart { message, .. } => {
                self.service_tier = common::claude_usage_to_openai_service_tier(&message.usage);
                vec![response_created(*message, self.service_tier.clone())]
            }
            claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block,
                ..
            } => self.content_block_start_to_response(index, *content_block),
            claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
                self.event_delta_to_response(index, *delta)
            }
            claude::KnownStreamEvent::MessageDelta { delta, usage, .. } => {
                let delta = *delta;
                if let Some(tier) = usage
                    .as_deref()
                    .and_then(common::claude_usage_to_openai_service_tier)
                {
                    self.service_tier = Some(tier);
                }
                let usage = common::claude_usage_to_completion_option(usage);
                let (status, incomplete_details) = delta
                    .stop_reason
                    .map(response_status_from_claude_stop)
                    .unwrap_or((openai::ResponseStatus::InProgress, None));
                vec![response_lifecycle_event(
                    "claude_msg".to_owned(),
                    common::default_openai_model(),
                    usage,
                    self.service_tier.clone(),
                    status,
                    incomplete_details,
                )]
            }
            claude::KnownStreamEvent::MessageStop { .. } => vec![response_lifecycle_event(
                "claude_msg".to_owned(),
                common::default_openai_model(),
                None,
                self.service_tier.clone(),
                openai::ResponseStatus::Completed,
                None,
            )],
            claude::KnownStreamEvent::Error { error, .. } => {
                vec![known(openai::KnownResponseStreamEvent::Error {
                    code: error.type_,
                    message: error.message,
                    param: String::new(),
                    sequence_number: None,
                    extra: Default::default(),
                })]
            }
            _ => Vec::new(),
        }
    }

    fn content_block_start_to_response(
        &mut self,
        index: u64,
        block: claude::ContentBlock,
    ) -> Vec<openai::ResponseStreamEvent> {
        let output_index = index_to_u32(index);
        match block {
            claude::ContentBlock::Text(block) => {
                if block.text.is_empty() {
                    Vec::new()
                } else {
                    vec![output_text_delta(output_index, block.text)]
                }
            }
            claude::ContentBlock::Thinking(block) => {
                self.reasoning_text
                    .entry(output_index)
                    .or_default()
                    .push_str(&block.thinking);
                if block.thinking.is_empty() {
                    if block.signature.is_empty() {
                        Vec::new()
                    } else {
                        vec![reasoning_done(output_index, None, block.signature)]
                    }
                } else {
                    let mut events =
                        vec![reasoning_text_delta(output_index, block.thinking.clone())];
                    if !block.signature.is_empty() {
                        events.push(reasoning_done(
                            output_index,
                            Some(block.thinking),
                            block.signature,
                        ));
                    }
                    events
                }
            }
            claude::ContentBlock::RedactedThinking(block) => {
                vec![reasoning_done(output_index, None, block.data)]
            }
            claude::ContentBlock::ToolUse(block) => {
                let item_id = common::response_function_call_item_id(&block.id);
                self.item_ids.insert(output_index, item_id.clone());
                vec![output_item_added(
                    output_index,
                    openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                        arguments: json_object_to_arguments(block.input),
                        call_id: common::response_call_id(&block.id),
                        name: block.name,
                        id: Some(item_id),
                        caller: None,
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::InProgress),
                        extra: Default::default(),
                    }),
                )]
            }
            claude::ContentBlock::McpToolUse(block) => {
                let item_id = common::response_function_call_item_id(&block.id);
                self.item_ids.insert(output_index, item_id.clone());
                vec![output_item_added(
                    output_index,
                    openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                        arguments: json_object_to_arguments(block.input),
                        call_id: common::response_call_id(&block.id),
                        name: block.name,
                        id: Some(item_id),
                        caller: None,
                        namespace: Some(block.server_name),
                        status: Some(openai::ResponseItemLifecycleStatus::InProgress),
                        extra: Default::default(),
                    }),
                )]
            }
            _ => Vec::new(),
        }
    }

    fn event_delta_to_response(
        &mut self,
        index: u64,
        delta: claude::EventDelta,
    ) -> Vec<openai::ResponseStreamEvent> {
        let output_index = index_to_u32(index);
        match delta {
            claude::EventDelta::Known(delta) => match *delta {
                claude::KnownEventDelta::Text { text, .. } => {
                    vec![output_text_delta(output_index, text)]
                }
                claude::KnownEventDelta::Thinking { thinking, .. } => {
                    self.reasoning_text
                        .entry(output_index)
                        .or_default()
                        .push_str(&thinking);
                    vec![reasoning_text_delta(output_index, thinking)]
                }
                claude::KnownEventDelta::Signature { signature, .. } => vec![reasoning_done(
                    output_index,
                    self.reasoning_text.get(&output_index).cloned(),
                    signature,
                )],
                claude::KnownEventDelta::InputJson { partial_json, .. } => vec![known(
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                        delta: partial_json,
                        item_id: self.item_id_for_index(output_index),
                        output_index,
                        sequence_number: None,
                        extra: Default::default(),
                    },
                )],
                claude::KnownEventDelta::Compaction { content, .. } => {
                    vec![output_text_delta(output_index, content)]
                }
                _ => Vec::new(),
            },
            claude::EventDelta::Unknown(_) => Vec::new(),
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        }
    }

    fn item_id_for_index(&self, output_index: u32) -> String {
        self.item_ids
            .get(&output_index)
            .cloned()
            .unwrap_or_else(|| common::indexed_response_function_call_item_id(output_index))
    }
}

fn response_created(
    message: claude::CreateMessageStartBody,
    service_tier: Option<openai::ServiceTier>,
) -> openai::ResponseStreamEvent {
    known(openai::KnownResponseStreamEvent::ResponseCreated {
        response: Box::new(response_object(
            message.id,
            common::claude_model_string(message.model).into(),
            Some(common::claude_usage_to_completion(message.usage)),
            service_tier,
            openai::ResponseStatus::InProgress,
            None,
        )),
        sequence_number: None,
        extra: Default::default(),
    })
}

fn output_text_delta(output_index: u32, text: String) -> openai::ResponseStreamEvent {
    known(openai::KnownResponseStreamEvent::ResponseOutputTextDelta {
        content_index: 0,
        delta: text,
        item_id: message_id(output_index),
        logprobs: None,
        output_index,
        sequence_number: None,
        extra: Default::default(),
    })
}

fn reasoning_text_delta(output_index: u32, text: String) -> openai::ResponseStreamEvent {
    known(
        openai::KnownResponseStreamEvent::ResponseReasoningTextDelta {
            content_index: 0,
            delta: text,
            item_id: reasoning_id(output_index),
            output_index,
            sequence_number: None,
            extra: Default::default(),
        },
    )
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

fn output_item_added(output_index: u32, item: openai::ResponseItem) -> openai::ResponseStreamEvent {
    known(openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
        item: Box::new(openai::ResponseOutputItem::new(item)),
        output_index,
        sequence_number: None,
        extra: Default::default(),
    })
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
    let response = Box::new(response_object(
        id,
        model,
        usage,
        service_tier,
        status,
        incomplete_details,
    ));

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

fn response_object(
    id: String,
    model: openai::OpenAiModelId,
    usage: Option<openai::CompletionUsage>,
    service_tier: Option<openai::ServiceTier>,
    status: openai::ResponseStatus,
    incomplete_details: Option<openai::IncompleteDetails>,
) -> openai::ResponseObject {
    crate::protocol::wire!(openai::ResponseObject {
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
        usage: common::completion_usage_to_response(usage),
        user: None,
        extra: Default::default(),
    })
}

fn response_status_from_claude_stop(
    reason: claude::StopReason,
) -> (openai::ResponseStatus, Option<openai::IncompleteDetails>) {
    match reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::MaxOutputTokens),
                extra: Default::default(),
            })),
        ),
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => (
            openai::ResponseStatus::Incomplete,
            Some(crate::protocol::wire!(openai::IncompleteDetails {
                reason: Some(openai::IncompleteReason::ContentFilter),
                extra: Default::default(),
            })),
        ),
        _ => (openai::ResponseStatus::Completed, None),
    }
}

fn json_object_to_arguments(value: claude::JsonObject) -> String {
    serde_json::to_string(&value).unwrap_or_default()
}

fn message_id(index: u32) -> String {
    format!("msg_{index}")
}

fn reasoning_id(index: u32) -> String {
    format!("reasoning_{index}")
}

fn index_to_u32(index: u64) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

fn known(event: openai::KnownResponseStreamEvent) -> openai::ResponseStreamEvent {
    openai::ResponseStreamEvent::Known(event)
}
