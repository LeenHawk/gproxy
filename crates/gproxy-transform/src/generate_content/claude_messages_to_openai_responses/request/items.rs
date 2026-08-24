use std::collections::BTreeMap;

use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items::{self, NativeKind};
use crate::common::responses;

use super::tool_output::{function_output, reasoning_item, redacted_reasoning_item, take_item_id};

pub(super) fn message_items(
    message: claude::MessageParam,
    native_calls: &mut BTreeMap<String, NativeKind>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let role = match &message.role {
        claude::MessageRole::Known(claude::MessageRoleKnown::User)
        | claude::MessageRole::Unknown(_) => openai::ResponseEasyInputMessageRole::User,
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            openai::ResponseEasyInputMessageRole::Assistant
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            openai::ResponseEasyInputMessageRole::Developer
        }
        _ => {
            return Err(TransformError::unsupported("Claude role", "future role"));
        }
    };
    let blocks = match message.content {
        claude::StringOrArray::String(text) => {
            vec![claude::ContentBlockParam::Text(claude::TextBlock {
                text,
                type_: claude::TextBlockType::Text,
                cache_control: None,
                citations: None,
                rest: Default::default(),
            })]
        }
        claude::StringOrArray::Array(blocks) => blocks,
        claude::StringOrArray::Raw(raw) => return Ok(vec![openai::ResponseItem::Unknown(raw)]),
        _ => {
            return Err(TransformError::unsupported(
                "Claude content",
                "future content shape",
            ));
        }
    };
    let mut output = Vec::new();
    let mut message_blocks = Vec::new();
    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(_)
            | claude::ContentBlockParam::Image(_)
            | claude::ContentBlockParam::Document(_)
            | claude::ContentBlockParam::Raw(_) => message_blocks.push(block),
            claude::ContentBlockParam::ToolUse(block) => {
                let call_id = block.id.clone();
                let (item, kind) = items::claude_call(
                    block.id,
                    block.input,
                    block.name,
                    block.rest,
                    openai::ResponseItemLifecycleStatus::Completed,
                )?;
                if let Some(kind) = kind {
                    native_calls.insert(call_id, kind);
                }
                output.push(openai::ResponseItem::Typed(Box::new(item)));
            }
            claude::ContentBlockParam::ToolResult(block) => {
                let item = if let Some(kind) = native_calls.get(&block.tool_use_id).copied() {
                    openai::ResponseItem::Typed(Box::new(items::claude_result(block, kind)?))
                } else {
                    function_output(block)?
                };
                output.push(item);
            }
            claude::ContentBlockParam::Thinking(block) => output.push(reasoning_item(block)?),
            claude::ContentBlockParam::RedactedThinking(block) => {
                output.push(redacted_reasoning_item(block)?);
            }
            claude::ContentBlockParam::Compaction(mut block) => {
                let encrypted_content = block.encrypted_content.ok_or_else(|| {
                    TransformError::shape("Claude compaction block", "encrypted_content is missing")
                })?;
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content,
                        id: take_item_id(&mut block.rest)?,
                        created_by: None,
                        rest: block.rest,
                    },
                )));
            }
            other => {
                return Err(TransformError::unsupported(
                    "Claude content block",
                    serde_json::to_string(&other)?,
                ));
            }
        }
    }
    if !message_blocks.is_empty() {
        let content = responses::claude_to_input(message_blocks)?;
        output.insert(
            0,
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
                openai::ResponseEasyInputMessageItem {
                    type_: Some(openai::ResponseMessageItemType::Message),
                    role,
                    content: openai::ResponseEasyInputContent::Parts(content),
                    phase: None,
                    rest: message.rest,
                },
            )),
        );
    }
    Ok(output)
}
