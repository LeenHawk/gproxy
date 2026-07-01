use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: openai::ChatCompletionChunk,
    _: &TransformContext,
) -> Result<claude::StreamEvent, TransformError> {
    Ok(chat_chunk_to_claude_event(input))
}

fn chat_chunk_to_claude_event(input: openai::ChatCompletionChunk) -> claude::StreamEvent {
    let id = input.id;
    let model = input.model;
    let usage = input.usage;

    let Some(choice) = input.choices.into_iter().next() else {
        return if usage.is_some() {
            message_delta(None, common::completion_usage_to_claude_box(usage))
        } else {
            ping()
        };
    };

    let index = u64::from(choice.index);
    let delta = choice.delta;

    if is_role_only_delta(&delta) {
        return known(claude::KnownStreamEvent::MessageStart {
            message: Box::new(claude::CreateMessageStartBody {
                id,
                type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
                role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
                content: Vec::new(),
                model: common::openai_model_string(model).into(),
                stop_reason: None,
                stop_sequence: None,
                usage: common::empty_claude_usage(),
                extra: Default::default(),
            }),
            extra: Default::default(),
        });
    }

    if let Some(content) = delta.content.filter(|value| !value.is_empty()) {
        return content_delta(index, claude_text_delta(content));
    }

    if let Some(reasoning) = delta.reasoning_content.filter(|value| !value.is_empty()) {
        return content_delta(index, claude_thinking_delta(reasoning));
    }

    if let Some(refusal) = delta.refusal.filter(|value| !value.is_empty()) {
        return content_delta(index, claude_text_delta(refusal));
    }

    if let Some(event) = delta
        .tool_calls
        .and_then(|tool_calls| tool_calls.into_iter().next())
        .map(chat_tool_delta_to_claude)
    {
        return event;
    }

    if let Some(function_call) = delta.function_call {
        if let Some(arguments) = function_call.arguments.filter(|value| !value.is_empty()) {
            return content_delta(index, claude_input_json_delta(arguments));
        }
        if let Some(name) = function_call.name.filter(|value| !value.is_empty()) {
            return tool_block_start(index, format!("call_{name}"), name);
        }
    }

    if let Some(finish_reason) = choice.finish_reason {
        return message_delta(
            Some(common::chat_finish_reason_to_claude(finish_reason)),
            common::completion_usage_to_claude_box(usage),
        );
    }

    ping()
}

fn is_role_only_delta(delta: &openai::ChatDelta) -> bool {
    matches!(delta.role, Some(openai::ChatDeltaRole::Assistant))
        && delta.content.as_deref().unwrap_or_default().is_empty()
        && delta
            .reasoning_content
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        && delta.refusal.as_deref().unwrap_or_default().is_empty()
        && delta.tool_calls.as_ref().is_none_or(Vec::is_empty)
        && delta.function_call.is_none()
}

fn chat_tool_delta_to_claude(call: openai::ChatToolCallDelta) -> claude::StreamEvent {
    let index = u64::from(call.index);
    if let Some(function) = call.function {
        if let Some(arguments) = function.arguments.filter(|value| !value.is_empty()) {
            return content_delta(index, claude_input_json_delta(arguments));
        }
        if let Some(name) = function.name.filter(|value| !value.is_empty()) {
            return tool_block_start(
                index,
                call.id.unwrap_or_else(|| format!("call_{index}")),
                name,
            );
        }
    }

    if let Some(custom) = call.custom {
        if let Some(input) = custom.input.filter(|value| !value.is_empty()) {
            return content_delta(index, claude_input_json_delta(input));
        }
        if let Some(name) = custom.name.filter(|value| !value.is_empty()) {
            return tool_block_start(
                index,
                call.id.unwrap_or_else(|| format!("call_{index}")),
                name,
            );
        }
    }

    ping()
}

fn tool_block_start(index: u64, id: String, name: String) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::ContentBlockStart {
        index,
        content_block: Box::new(claude::ContentBlock::ToolUse(
            claude::ResponseToolUseBlock {
                id,
                input: Default::default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            },
        )),
        extra: Default::default(),
    })
}

fn content_delta(index: u64, delta: claude::KnownEventDelta) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::ContentBlockDelta {
        index,
        delta: Box::new(claude::EventDelta::Known(Box::new(delta))),
        extra: Default::default(),
    })
}

fn claude_text_delta(text: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Text {
        text,
        extra: Default::default(),
    }
}

fn claude_thinking_delta(thinking: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::Thinking {
        estimated_tokens: None,
        thinking,
        extra: Default::default(),
    }
}

fn claude_input_json_delta(partial_json: String) -> claude::KnownEventDelta {
    claude::KnownEventDelta::InputJson {
        partial_json,
        extra: Default::default(),
    }
}

fn message_delta(
    stop_reason: Option<claude::StopReason>,
    usage: Option<Box<claude::Usage>>,
) -> claude::StreamEvent {
    known(claude::KnownStreamEvent::MessageDelta {
        context_management: None,
        delta: Box::new(claude::MessageDelta {
            container: None,
            stop_reason,
            stop_sequence: None,
            stop_details: None,
            extra: Default::default(),
        }),
        usage,
        extra: Default::default(),
    })
}

fn ping() -> claude::StreamEvent {
    known(claude::KnownStreamEvent::Ping {
        extra: Default::default(),
    })
}

fn known(event: claude::KnownStreamEvent) -> claude::StreamEvent {
    claude::StreamEvent::Known(Box::new(event))
}
