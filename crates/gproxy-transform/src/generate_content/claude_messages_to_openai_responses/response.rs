use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let model = input
        .model
        .as_ref()
        .map(crate::models::common::wire_string)
        .transpose()?
        .ok_or_else(|| TransformError::shape("OpenAI Responses response", "model is missing"))?
        .into();
    let usage = usage::responses_to_claude(input.usage)
        .ok_or_else(|| TransformError::shape("OpenAI Responses response", "usage is missing"))?;
    let mut rest = input.rest;
    preserve_number(&mut rest, "openai_created_at", input.created_at);
    preserve_number(&mut rest, "openai_completed_at", input.completed_at);
    let mut blocks = Vec::new();
    for item in input.output {
        blocks.extend(item_blocks(item)?);
    }
    let has_tool = blocks
        .iter()
        .any(|block| matches!(block, claude::ResponseContentBlock::ToolUse(_)));
    let stop_reason = match input.status {
        Some(openai::ResponseStatus::Incomplete) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        Some(openai::ResponseStatus::Failed) => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
        _ if has_tool => claude::StopReason::Known(claude::StopReasonKnown::ToolUse),
        _ => claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
    };
    let output = claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: blocks,
        model,
        stop_reason,
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

fn item_blocks(
    item: openai::ResponseItem,
) -> Result<Vec<claude::ResponseContentBlock>, TransformError> {
    match item {
        openai::ResponseItem::Message(message) => match message {
            openai::ResponseMessageItem::Output(message) => {
                let item_id = message.id;
                let mut blocks = message
                    .content
                    .into_iter()
                    .map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                                citations: None,
                                text: part.text,
                                type_: claude::TextBlockType::Text,
                                rest: part.rest,
                            })
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                                citations: None,
                                text: part.refusal,
                                type_: claude::TextBlockType::Text,
                                rest: part.rest,
                            })
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(raw) => {
                            claude::ResponseContentBlock::Raw(raw)
                        }
                    })
                    .collect::<Vec<_>>();
                if let Some(rest) = blocks.iter_mut().find_map(block_rest_mut) {
                    rest.insert("openai_item_id".into(), item_id.into());
                }
                Ok(blocks)
            }
            openai::ResponseMessageItem::Unknown(raw) => {
                Ok(vec![claude::ResponseContentBlock::Raw(raw)])
            }
            unsupported @ (openai::ResponseMessageItem::Input(_)
            | openai::ResponseMessageItem::EasyInput(_)) => {
                Ok(vec![claude::ResponseContentBlock::Raw(
                    serde_json::to_value(unsupported)?,
                )])
            }
        },
        openai::ResponseItem::Typed(item) => typed_blocks(*item),
        openai::ResponseItem::Unknown(raw) => Ok(vec![claude::ResponseContentBlock::Raw(raw)]),
    }
}

fn typed_blocks(
    item: openai::TypedResponseItem,
) -> Result<Vec<claude::ResponseContentBlock>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            mut rest,
            ..
        } => {
            preserve_string(&mut rest, "openai_item_id", id);
            vec![claude::ResponseContentBlock::ToolUse(
                claude::ResponseToolUseBlock {
                    id: call_id,
                    input: serde_json::from_str(&arguments)?,
                    name,
                    type_: claude::ToolUseBlockType::ToolUse,
                    caller: None,
                    rest,
                },
            )]
        }
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            mut rest,
            ..
        } => {
            preserve_string(&mut rest, "openai_item_id", id);
            vec![claude::ResponseContentBlock::ToolUse(
                claude::ResponseToolUseBlock {
                    id: call_id,
                    input: serde_json::from_str(&input)?,
                    name,
                    type_: claude::ToolUseBlockType::ToolUse,
                    caller: None,
                    rest,
                },
            )]
        }
        openai::TypedResponseItem::Reasoning {
            id,
            content,
            encrypted_content,
            mut rest,
            ..
        } => {
            preserve_string(&mut rest, "openai_item_id", id);
            let thinking = content
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .collect::<String>();
            vec![claude::ResponseContentBlock::Thinking(
                claude::ThinkingBlock {
                    signature: encrypted_content,
                    thinking,
                    type_: claude::ThinkingBlockType::Thinking,
                    rest,
                },
            )]
        }
        openai::TypedResponseItem::Compaction {
            encrypted_content,
            id,
            mut rest,
            ..
        } => {
            preserve_string(&mut rest, "openai_item_id", id);
            vec![claude::ResponseContentBlock::Compaction(
                claude::ResponseCompactionBlock {
                    content: None,
                    encrypted_content,
                    type_: claude::CompactionBlockType::Compaction,
                    rest,
                },
            )]
        }
        other => {
            if let Some(call) = items::openai_call(other.clone())? {
                vec![items::response_block(call)]
            } else {
                vec![claude::ResponseContentBlock::Raw(serde_json::to_value(
                    other,
                )?)]
            }
        }
    })
}

fn block_rest_mut(
    block: &mut claude::ResponseContentBlock,
) -> Option<&mut serde_json::Map<String, serde_json::Value>> {
    match block {
        claude::ResponseContentBlock::Text(block) => Some(&mut block.rest),
        _ => None,
    }
}

fn preserve_number(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: Option<u64>,
) {
    if let Some(value) = value {
        rest.insert(name.into(), value.into());
    }
}

fn preserve_string(
    rest: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        rest.insert(name.into(), value.into());
    }
}
