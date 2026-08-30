use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let choice = input
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "choice is missing"))?;
    let mut blocks = Vec::new();
    if let Some(reasoning) = choice
        .message
        .reasoning_content
        .filter(|value| !value.is_empty())
    {
        blocks.push(claude::ResponseContentBlock::Thinking(
            claude::ThinkingBlock {
                signature: None,
                thinking: reasoning,
                type_: claude::ThinkingBlockType::Thinking,
                rest: Default::default(),
            },
        ));
    }
    if let Some(text) = choice.message.content.filter(|value| !value.is_empty()) {
        blocks.push(claude::ResponseContentBlock::Text(
            claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            },
        ));
    }
    if let Some(calls) = choice.message.tool_calls {
        for call in calls {
            match call {
                openai::ChatToolCall::Function(call) => {
                    blocks.push(claude::ResponseContentBlock::ToolUse(
                        claude::ResponseToolUseBlock {
                            id: call.id,
                            input: serde_json::from_str(&call.function.arguments)
                                .unwrap_or_default(),
                            name: call.function.name,
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            rest: merge(call.rest, call.function.rest),
                        },
                    ));
                }
                openai::ChatToolCall::Custom(call) => {
                    blocks.push(claude::ResponseContentBlock::ToolUse(
                        claude::ResponseToolUseBlock {
                            id: call.id,
                            input: serde_json::from_str(&call.custom.input).unwrap_or_default(),
                            name: call.custom.name,
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            rest: merge(call.rest, call.custom.rest),
                        },
                    ));
                }
                openai::ChatToolCall::Unknown(raw) => {
                    blocks.push(claude::ResponseContentBlock::Raw(raw));
                }
            }
        }
    }
    if let Some(raw) = choice.message.rest.get("claude_content_blocks") {
        for value in raw.as_array().into_iter().flatten() {
            blocks.push(serde_json::from_value(value.clone())?);
        }
    }
    let usage = usage::chat_to_claude(input.usage)
        .ok_or_else(|| TransformError::shape("OpenAI Chat response", "usage is missing"))?;
    let mut rest = input.rest;
    if let Some(created) = input.created {
        rest.insert("openai_created".into(), created.into());
    }
    let output = claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: blocks,
        model: crate::models::common::wire_string(&input.model)?.into(),
        stop_reason: stop::chat_to_claude(choice.finish_reason),
        stop_sequence: None,
        usage,
        container: None,
        context_management: None,
        diagnostics: None,
        stop_details: None,
        rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn merge(
    mut left: serde_json::Map<String, serde_json::Value>,
    right: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    left.extend(right);
    left
}
