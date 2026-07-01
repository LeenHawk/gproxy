use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

pub fn stream_event(
    input: claude::StreamEvent,
    _: &TransformContext,
) -> Result<openai::ChatCompletionChunk, TransformError> {
    Ok(match input {
        claude::StreamEvent::Known(event) => known_event_to_chat(*event),
        claude::StreamEvent::Unknown(_) => empty_chunk(),
    })
}

fn known_event_to_chat(event: claude::KnownStreamEvent) -> openai::ChatCompletionChunk {
    match event {
        claude::KnownStreamEvent::MessageStart { message, .. } => message_start_to_chat(*message),
        claude::KnownStreamEvent::ContentBlockStart {
            index,
            content_block,
            ..
        } => content_block_start_to_chat(index, *content_block),
        claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
            event_delta_to_chat(index, *delta)
        }
        claude::KnownStreamEvent::MessageDelta { delta, usage, .. } => {
            let delta = *delta;
            let usage = common::claude_usage_to_completion_option(usage);
            if let Some(reason) = delta.stop_reason {
                common::chat_finish_chunk(
                    "claude_msg".to_owned(),
                    common::default_openai_model(),
                    0,
                    common::claude_stop_reason_to_chat(reason),
                    usage,
                )
            } else {
                common::empty_chat_chunk(
                    "claude_msg".to_owned(),
                    common::default_openai_model(),
                    0,
                    usage,
                )
            }
        }
        claude::KnownStreamEvent::Error { .. } => common::chat_finish_chunk(
            "claude_error".to_owned(),
            common::default_openai_model(),
            0,
            openai::ChatFinishReason::ContentFilter,
            None,
        ),
        _ => empty_chunk(),
    }
}

fn message_start_to_chat(message: claude::CreateMessageStartBody) -> openai::ChatCompletionChunk {
    let mut delta = common::empty_chat_delta();
    delta.role = Some(openai::ChatDeltaRole::Assistant);
    common::chat_delta_chunk(
        message.id,
        common::claude_model_string(message.model).into(),
        0,
        0,
        delta,
        None,
        Some(common::claude_usage_to_completion(message.usage)),
    )
}

fn content_block_start_to_chat(
    index: u64,
    block: claude::ContentBlock,
) -> openai::ChatCompletionChunk {
    match block {
        claude::ContentBlock::Text(block) => {
            if block.text.is_empty() {
                empty_chunk()
            } else {
                common::chat_text_delta(
                    "claude_text".to_owned(),
                    common::default_openai_model(),
                    0,
                    0,
                    block.text,
                )
            }
        }
        claude::ContentBlock::Thinking(block) => {
            if block.thinking.is_empty() {
                empty_chunk()
            } else {
                common::chat_reasoning_delta(
                    "claude_thinking".to_owned(),
                    common::default_openai_model(),
                    0,
                    0,
                    block.thinking,
                )
            }
        }
        claude::ContentBlock::ToolUse(block) => tool_start_to_chat(
            index_to_u32(index),
            block.id,
            block.name,
            json_object_to_arguments(block.input),
        ),
        claude::ContentBlock::McpToolUse(block) => tool_start_to_chat(
            index_to_u32(index),
            block.id,
            block.name,
            json_object_to_arguments(block.input),
        ),
        _ => empty_chunk(),
    }
}

fn event_delta_to_chat(index: u64, delta: claude::EventDelta) -> openai::ChatCompletionChunk {
    let index = index_to_u32(index);
    match delta {
        claude::EventDelta::Known(delta) => match *delta {
            claude::KnownEventDelta::Text { text, .. } => common::chat_text_delta(
                "claude_text".to_owned(),
                common::default_openai_model(),
                0,
                0,
                text,
            ),
            claude::KnownEventDelta::Thinking { thinking, .. } => common::chat_reasoning_delta(
                "claude_thinking".to_owned(),
                common::default_openai_model(),
                0,
                0,
                thinking,
            ),
            claude::KnownEventDelta::InputJson { partial_json, .. } => {
                let mut chat_delta = common::empty_chat_delta();
                chat_delta.tool_calls = Some(vec![common::chat_function_tool_delta(
                    index,
                    None,
                    None,
                    Some(partial_json),
                )]);
                common::chat_delta_chunk(
                    "claude_tool".to_owned(),
                    common::default_openai_model(),
                    0,
                    0,
                    chat_delta,
                    None,
                    None,
                )
            }
            claude::KnownEventDelta::Compaction { content, .. } => common::chat_text_delta(
                "claude_compaction".to_owned(),
                common::default_openai_model(),
                0,
                0,
                content,
            ),
            _ => empty_chunk(),
        },
        claude::EventDelta::Unknown(_) => empty_chunk(),
    }
}

fn tool_start_to_chat(
    index: u32,
    id: String,
    name: String,
    arguments: Option<String>,
) -> openai::ChatCompletionChunk {
    let mut delta = common::empty_chat_delta();
    delta.tool_calls = Some(vec![common::chat_function_tool_delta(
        index,
        Some(id.clone()),
        Some(name),
        arguments,
    )]);
    common::chat_delta_chunk(id, common::default_openai_model(), 0, 0, delta, None, None)
}

fn json_object_to_arguments(value: claude::JsonObject) -> Option<String> {
    (!value.is_empty()).then(|| serde_json::to_string(&value).unwrap_or_default())
}

fn empty_chunk() -> openai::ChatCompletionChunk {
    common::empty_chat_chunk(
        "claude_event".to_owned(),
        common::default_openai_model(),
        0,
        None,
    )
}

fn index_to_u32(index: u64) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}
