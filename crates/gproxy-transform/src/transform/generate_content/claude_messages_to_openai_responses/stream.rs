use std::collections::BTreeMap;

use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::claude_usage_to_response;
use crate::transform::compact::claude_to_openai::{
    prepare_response_output_item, server_tool_call, typed_tool_call,
};

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
    buffered_tools: BTreeMap<u32, BufferedTool>,
    service_tier: Option<openai::ServiceTier>,
    terminal_emitted: bool,
}

struct BufferedTool {
    id: String,
    input: String,
    kind: BufferedToolKind,
}

enum BufferedToolKind {
    Client(String),
    Server(claude::ServerToolUseName),
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
            claude::KnownStreamEvent::ContentBlockStop { index, .. } => {
                self.content_block_stop_to_response(index)
            }
            claude::KnownStreamEvent::MessageDelta { delta, usage, .. } => {
                let delta = *delta;
                if let Some(tier) = usage
                    .as_deref()
                    .and_then(common::claude_usage_to_openai_service_tier)
                {
                    self.service_tier = Some(tier);
                }
                let usage = usage.map(|usage| claude_usage_to_response(*usage));
                let stop_reason = delta.stop_reason;
                let terminal = stop_reason.is_some();
                let (status, incomplete_details) = stop_reason
                    .map(response_status_from_claude_stop)
                    .unwrap_or((openai::ResponseStatus::InProgress, None));
                self.terminal_emitted |= terminal;
                vec![response_lifecycle_event(
                    "claude_msg".to_owned(),
                    common::default_openai_model(),
                    usage,
                    self.service_tier.clone(),
                    status,
                    incomplete_details,
                )]
            }
            claude::KnownStreamEvent::MessageStop { .. } => {
                if self.terminal_emitted {
                    Vec::new()
                } else {
                    self.terminal_emitted = true;
                    vec![response_lifecycle_event(
                        "claude_msg".to_owned(),
                        common::default_openai_model(),
                        None,
                        self.service_tier.clone(),
                        openai::ResponseStatus::Completed,
                        None,
                    )]
                }
            }
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
                if is_approximate_client_tool(&block.name) {
                    self.buffered_tools.insert(
                        output_index,
                        BufferedTool {
                            id: block.id,
                            input: initial_tool_input(block.input),
                            kind: BufferedToolKind::Client(block.name),
                        },
                    );
                    return Vec::new();
                }
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
            claude::ContentBlock::ServerToolUse(block) => {
                self.buffered_tools.insert(
                    output_index,
                    BufferedTool {
                        id: block.id,
                        input: initial_tool_input(block.input),
                        kind: BufferedToolKind::Server(block.name),
                    },
                );
                Vec::new()
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
                    if let Some(tool) = self.buffered_tools.get_mut(&output_index) {
                        tool.input.push_str(&partial_json);
                        return Vec::new();
                    } else {
                        openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta {
                            delta: partial_json,
                            item_id: self.item_id_for_index(output_index),
                            output_index,
                            sequence_number: None,
                            extra: Default::default(),
                        }
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

    fn content_block_stop_to_response(&mut self, index: u64) -> Vec<openai::ResponseStreamEvent> {
        let output_index = index_to_u32(index);
        let Some(tool) = self.buffered_tools.remove(&output_index) else {
            return Vec::new();
        };
        let input = serde_json::from_str(&tool.input).unwrap_or_default();
        let completed = match tool.kind {
            BufferedToolKind::Client(name) => typed_tool_call(tool.id, input, name).0,
            BufferedToolKind::Server(name) => server_tool_call(tool.id, input, name),
        };
        let mut in_progress = completed.clone();
        set_item_status(
            &mut in_progress,
            openai::ResponseItemLifecycleStatus::InProgress,
        );
        vec![
            output_item_added(output_index, openai::ResponseItem::Typed(in_progress)),
            output_item_done(output_index, openai::ResponseItem::Typed(completed)),
        ]
    }
}

fn is_approximate_client_tool(name: &str) -> bool {
    matches!(
        name,
        "bash" | "str_replace_editor" | "str_replace_based_edit_tool"
    )
}

fn initial_tool_input(input: claude::JsonObject) -> String {
    if input.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&input).unwrap_or_default()
    }
}

fn set_item_status(
    item: &mut openai::TypedResponseItem,
    status: openai::ResponseItemLifecycleStatus,
) {
    match item {
        openai::TypedResponseItem::FunctionCall { status: value, .. }
        | openai::TypedResponseItem::ShellCall { status: value, .. }
        | openai::TypedResponseItem::ToolSearchCall { status: value, .. } => *value = Some(status),
        openai::TypedResponseItem::ApplyPatchCall { status: value, .. } => {
            *value = match status {
                openai::ResponseItemLifecycleStatus::InProgress => {
                    openai::ResponseApplyPatchCallStatus::InProgress
                }
                _ => openai::ResponseApplyPatchCallStatus::Completed,
            }
        }
        openai::TypedResponseItem::WebSearchCall { status: value, .. } => {
            *value = match status {
                openai::ResponseItemLifecycleStatus::InProgress => {
                    openai::ResponseWebSearchCallStatus::InProgress
                }
                _ => openai::ResponseWebSearchCallStatus::Completed,
            }
        }
        _ => {}
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
            Some(claude_usage_to_response(message.usage)),
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

fn output_item_added(
    output_index: u32,
    mut item: openai::ResponseItem,
) -> openai::ResponseStreamEvent {
    if let openai::ResponseItem::Typed(item) = &mut item {
        prepare_response_output_item(item);
    }
    known(openai::KnownResponseStreamEvent::ResponseOutputItemAdded {
        item: Box::new(openai::ResponseOutputItem::new(item)),
        output_index,
        sequence_number: None,
        extra: Default::default(),
    })
}

fn output_item_done(
    output_index: u32,
    mut item: openai::ResponseItem,
) -> openai::ResponseStreamEvent {
    if let openai::ResponseItem::Typed(item) = &mut item {
        prepare_response_output_item(item);
    }
    known(openai::KnownResponseStreamEvent::ResponseOutputItemDone {
        item: Box::new(openai::ResponseOutputItem::new(item)),
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
    usage: Option<openai::ResponseUsage>,
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
        usage,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    #[test]
    fn streamed_bash_call_becomes_typed_shell_item() {
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );
        let mut transform = StreamTransform::default();
        transform
            .push(
                known_claude(claude::KnownStreamEvent::ContentBlockStart {
                    index: 0,
                    content_block: Box::new(claude::ContentBlock::ToolUse(crate::protocol::wire!(
                        claude::ResponseToolUseBlock {
                            id: "toolu_shell".to_owned(),
                            input: Default::default(),
                            name: "bash".to_owned(),
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            extra: Default::default(),
                        }
                    ))),
                    extra: Default::default(),
                }),
                &ctx,
            )
            .unwrap();
        transform
            .push(
                known_claude(claude::KnownStreamEvent::ContentBlockDelta {
                    index: 0,
                    delta: Box::new(claude::EventDelta::Known(Box::new(
                        claude::KnownEventDelta::InputJson {
                            partial_json: r#"{"command":"pwd"}"#.to_owned(),
                            extra: Default::default(),
                        },
                    ))),
                    extra: Default::default(),
                }),
                &ctx,
            )
            .unwrap();
        let events = transform
            .push(
                known_claude(claude::KnownStreamEvent::ContentBlockStop {
                    index: 0,
                    extra: Default::default(),
                }),
                &ctx,
            )
            .unwrap();

        assert_eq!(events.len(), 2);
        let openai::ResponseStreamEvent::Known(
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded { item, .. },
        ) = &events[0]
        else {
            panic!("expected output item added");
        };
        assert!(matches!(
            &item.0,
            openai::ResponseItem::Typed(openai::TypedResponseItem::ShellCall { .. })
        ));
    }

    #[test]
    fn message_stop_does_not_duplicate_terminal_from_message_delta() {
        let ctx = TransformContext::new(
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
        );
        let mut transform = StreamTransform::default();
        let terminal = transform
            .push(
                known_claude(claude::KnownStreamEvent::MessageDelta {
                    delta: Box::new(crate::protocol::wire!(claude::MessageDelta {
                        container: None,
                        stop_reason: Some(claude::StopReason::Known(
                            claude::StopReasonKnown::ToolUse,
                        )),
                        stop_sequence: None,
                        stop_details: None,
                        extra: Default::default(),
                    })),
                    usage: None,
                    context_management: None,
                    extra: Default::default(),
                }),
                &ctx,
            )
            .unwrap();
        assert!(matches!(
            terminal.as_slice(),
            [openai::ResponseStreamEvent::Known(
                openai::KnownResponseStreamEvent::ResponseCompleted { .. }
            )]
        ));

        let stop = transform
            .push(
                known_claude(claude::KnownStreamEvent::MessageStop {
                    extra: Default::default(),
                }),
                &ctx,
            )
            .unwrap();
        assert!(stop.is_empty());
    }

    fn known_claude(event: claude::KnownStreamEvent) -> claude::StreamEvent {
        claude::StreamEvent::Known(Box::new(event))
    }
}
