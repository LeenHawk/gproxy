use std::collections::BTreeMap;

use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items::{self, NativeKind};
use crate::common::responses;

use super::tool_output::{function_output, reasoning_item, redacted_reasoning_item};

pub(super) fn message_items(
    message: claude::MessageParam,
    native_calls: &mut BTreeMap<String, NativeKind>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let role = match &message.role {
        claude::MessageRole::Known(claude::MessageRoleKnown::User) => {
            openai::ResponseEasyInputMessageRole::User
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            openai::ResponseEasyInputMessageRole::Assistant
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            openai::ResponseEasyInputMessageRole::Developer
        }
        claude::MessageRole::Unknown(value) => {
            return Err(TransformError::unsupported("Claude role", value.clone()));
        }
        _ => {
            return Err(TransformError::unsupported("Claude role", "future role"));
        }
    };
    let blocks = match message.content {
        claude::StringOrArray::String(text) => {
            vec![claude::ContentBlockParam::Text(crate::wire!(
                claude::TextBlock {
                    text,
                    type_: claude::TextBlockType::Text,
                    cache_control: None,
                    citations: None,
                    rest: Default::default(),
                }
            ))]
        }
        claude::StringOrArray::Array(blocks) => blocks,
        claude::StringOrArray::Raw(_) => return Ok(Vec::new()),
        _ => {
            return Err(TransformError::unsupported(
                "Claude content",
                "future content shape",
            ));
        }
    };
    let mut output = Vec::new();
    let mut message_blocks = Vec::new();
    let mut output_parts = Vec::new();
    let assistant = role == openai::ResponseEasyInputMessageRole::Assistant;
    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) if assistant => {
                output_parts.push(openai::ResponseMessageOutputContentPart::OutputText(
                    crate::wire!(openai::ResponseOutputText {
                        type_: openai::ResponseOutputTextType::OutputText,
                        annotations: Vec::new(),
                        logprobs: None,
                        text: block.text,
                        rest: Default::default(),
                    }),
                ));
            }
            claude::ContentBlockParam::Image(_) | claude::ContentBlockParam::Document(_)
                if assistant => {}
            claude::ContentBlockParam::Text(_)
            | claude::ContentBlockParam::Image(_)
            | claude::ContentBlockParam::Document(_) => message_blocks.push(block),
            claude::ContentBlockParam::Raw(_) => {}
            claude::ContentBlockParam::ToolUse(block) => {
                let call_id = block.id.clone();
                let (item, kind) = items::claude_call(
                    block.id,
                    block.input,
                    block.name,
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
            claude::ContentBlockParam::Compaction(block) => {
                let encrypted_content = block.encrypted_content.ok_or_else(|| {
                    TransformError::shape("Claude compaction block", "encrypted_content is missing")
                })?;
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content,
                        id: None,
                        created_by: None,
                        rest: Default::default(),
                    },
                )));
            }
            claude::ContentBlockParam::ServerToolUse(block) => {
                let name = crate::models::common::wire_string(&block.name)?;
                let (item, kind) = items::claude_call(
                    block.id.clone(),
                    block.input,
                    name,
                    openai::ResponseItemLifecycleStatus::Completed,
                )?;
                if let Some(kind) = kind {
                    native_calls.insert(block.id, kind);
                }
                output.push(openai::ResponseItem::Typed(Box::new(item)));
            }
            claude::ContentBlockParam::McpToolUse(block) => {
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::McpCall {
                        id: block.id,
                        arguments: serde_json::to_string(&block.input)?,
                        name: block.name,
                        server_label: block.server_name,
                        approval_request_id: None,
                        error: None,
                        output: None,
                        status: Some(openai::ResponseMcpCallStatus::Completed),
                        rest: Default::default(),
                    },
                )));
            }
            claude::ContentBlockParam::AdvisorToolResult(block) => output.push(result_item(
                block.tool_use_id,
                serde_json::to_string(&block.content)?,
            )),
            claude::ContentBlockParam::CodeExecutionToolResult(block) => output.push(result_item(
                block.tool_use_id,
                serde_json::to_string(&block.content)?,
            )),
            claude::ContentBlockParam::BashCodeExecutionToolResult(block) => output.push(
                result_item(block.tool_use_id, serde_json::to_string(&block.content)?),
            ),
            claude::ContentBlockParam::TextEditorCodeExecutionToolResult(block) => output.push(
                result_item(block.tool_use_id, serde_json::to_string(&block.content)?),
            ),
            claude::ContentBlockParam::ToolSearchToolResult(block) => output.push(result_item(
                block.tool_use_id,
                serde_json::to_string(&block.content)?,
            )),
            claude::ContentBlockParam::McpToolResult(block) => output.push(result_item(
                block.tool_use_id,
                serde_json::to_string(&block.content)?,
            )),
            claude::ContentBlockParam::WebSearchToolResult(_)
            | claude::ContentBlockParam::WebFetchToolResult(_)
            | claude::ContentBlockParam::SearchResult(_)
            | claude::ContentBlockParam::ContainerUpload(_)
            | claude::ContentBlockParam::MidConversationSystem(_)
            | claude::ContentBlockParam::ToolAddition(_)
            | claude::ContentBlockParam::ToolRemoval(_)
            | claude::ContentBlockParam::Fallback(_) => {}
            _future => {}
        }
    }
    if !message_blocks.is_empty() || !output_parts.is_empty() {
        let content = if assistant {
            openai::ResponseEasyInputContent::OutputParts(output_parts)
        } else {
            openai::ResponseEasyInputContent::Parts(responses::claude_to_input(message_blocks)?)
        };
        output.insert(
            0,
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(crate::wire!(
                openai::ResponseEasyInputMessageItem {
                    type_: Some(openai::ResponseMessageItemType::Message),
                    role,
                    content,
                    phase: None,
                    rest: Default::default(),
                }
            ))),
        );
    }
    Ok(output)
}

fn result_item(call_id: String, output: String) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCallOutput {
        call_id,
        output: openai::ResponseOutput::Text(output),
        id: None,
        caller: None,
        name: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        rest: Default::default(),
    }))
}
