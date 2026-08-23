use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::{items, results, shape};
use crate::common::responses;

use super::{message, preserve_item_id};

pub(super) fn typed_item(
    item: openai::TypedResponseItem,
) -> Result<claude::MessageParam, TransformError> {
    match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            mut rest,
            ..
        } => {
            preserve_item_id(&mut rest, id);
            Ok(message(
                claude::MessageRoleKnown::Assistant,
                vec![tool_use(call_id, name, arguments, rest)?],
                Default::default(),
            ))
        }
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            mut rest,
            ..
        } => {
            preserve_item_id(&mut rest, id);
            Ok(message(
                claude::MessageRoleKnown::Assistant,
                vec![tool_use(call_id, name, input, rest)?],
                Default::default(),
            ))
        }
        openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            id,
            mut rest,
            ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id,
            output,
            id,
            mut rest,
            ..
        } => {
            preserve_item_id(&mut rest, id);
            Ok(message(
                claude::MessageRoleKnown::User,
                vec![claude::ContentBlockParam::ToolResult(
                    claude::ToolResultBlock {
                        tool_use_id: call_id,
                        type_: claude::ToolResultBlockType::ToolResult,
                        cache_control: None,
                        content: Some(response_output(output)?),
                        is_error: None,
                        rest,
                    },
                )],
                Default::default(),
            ))
        }
        openai::TypedResponseItem::Reasoning {
            id,
            content,
            encrypted_content,
            mut rest,
            ..
        } => {
            preserve_item_id(&mut rest, id);
            let thinking = content
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .collect::<String>();
            Ok(message(
                claude::MessageRoleKnown::Assistant,
                vec![claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                    signature: encrypted_content,
                    thinking,
                    type_: claude::ThinkingBlockType::Thinking,
                    rest,
                })],
                Default::default(),
            ))
        }
        openai::TypedResponseItem::Compaction {
            encrypted_content,
            id,
            mut rest,
            ..
        } => {
            preserve_item_id(&mut rest, id);
            Ok(message(
                claude::MessageRoleKnown::Assistant,
                vec![claude::ContentBlockParam::Compaction(
                    claude::CompactionBlock {
                        content: None,
                        encrypted_content: Some(encrypted_content),
                        type_: claude::CompactionBlockType::Compaction,
                        cache_control: None,
                        rest,
                    },
                )],
                Default::default(),
            ))
        }
        other => {
            if let Some(call) = items::openai_call(other.clone())? {
                return Ok(message(
                    claude::MessageRoleKnown::Assistant,
                    vec![items::request_block(call)],
                    Default::default(),
                ));
            }
            if let Some(result) = results::openai_result(other.clone()) {
                return Ok(message(
                    claude::MessageRoleKnown::User,
                    vec![results::result_block(result)],
                    Default::default(),
                ));
            }
            Err(TransformError::unsupported(
                "OpenAI Responses item",
                serde_json::to_string(&other)?,
            ))
        }
    }
}

fn tool_use(
    id: String,
    name: String,
    arguments: String,
    rest: openai::Rest,
) -> Result<claude::ContentBlockParam, TransformError> {
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input: shape::arguments_object(&arguments)?,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest,
    }))
}

fn response_output(
    output: openai::ResponseOutput,
) -> Result<claude::ToolResultContent, TransformError> {
    match output {
        openai::ResponseOutput::Text(text) => Ok(claude::ToolResultContent::Text(text)),
        openai::ResponseOutput::Parts(parts) => {
            let blocks = responses::input_to_claude(parts)?;
            let blocks = blocks
                .into_iter()
                .map(|block| serde_json::from_value(serde_json::to_value(block)?))
                .collect::<Result<_, serde_json::Error>>()?;
            Ok(claude::ToolResultContent::Blocks(blocks))
        }
        openai::ResponseOutput::Unknown(raw) => Ok(claude::ToolResultContent::Raw(raw)),
    }
}
