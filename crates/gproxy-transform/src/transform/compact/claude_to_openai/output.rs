use crate::protocol::{claude, openai};
use crate::transform::TransformContext;

use super::DEFAULT_REASONING_ID;
use super::tools::{
    compact_function_call_output_item, compact_server_tool_use_item,
    response_mcp_tool_result_content_to_text, server_tool_result_output,
};
use super::util::{claude_usage_to_openai, json_object_to_string};

pub fn response(
    input: claude::CreateMessageResponseBody,
    _: &TransformContext,
) -> openai::CompactedResponseObject {
    openai::CompactedResponseObject {
        id: input.id.clone(),
        created_at: 0,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output: claude_content_to_compact_output(input.id, input.content, &input.stop_reason),
        usage: claude_usage_to_openai(input.usage),
        extra: Default::default(),
    }
}

fn claude_content_to_compact_output(
    id: String,
    content: Vec<claude::ContentBlock>,
    stop_reason: &claude::StopReason,
) -> Vec<openai::CompactResponseItem> {
    let mut message_parts = Vec::new();
    let mut output = Vec::new();

    for block in content {
        match block {
            claude::ContentBlock::Text(block) => {
                message_parts.push(openai::CompactMessageContentPart::Text(
                    openai::CompactTextContent {
                        text: block.text,
                        type_: openai::CompactTextContentType::Text,
                        extra: Default::default(),
                    },
                ));
            }
            claude::ContentBlock::Thinking(block) => {
                output.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::Reasoning {
                        id: block.signature,
                        summary: Vec::new(),
                        content: Some(vec![openai::ResponseReasoningTextPart {
                            text: block.thinking,
                            type_: openai::ResponseReasoningTextType::ReasoningText,
                            extra: Default::default(),
                        }]),
                        encrypted_content: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: Default::default(),
                    },
                ));
            }
            claude::ContentBlock::RedactedThinking(block) => {
                output.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::Reasoning {
                        id: DEFAULT_REASONING_ID.to_owned(),
                        summary: Vec::new(),
                        content: None,
                        encrypted_content: Some(block.data),
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: Default::default(),
                    },
                ));
            }
            claude::ContentBlock::ToolUse(block) => {
                output.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::FunctionCall {
                        arguments: json_object_to_string(&block.input),
                        call_id: block.id.clone(),
                        name: block.name,
                        id: Some(block.id),
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: Default::default(),
                    },
                ));
            }
            claude::ContentBlock::ServerToolUse(block) => {
                output.push(compact_server_tool_use_item(
                    block.id,
                    block.input,
                    block.name,
                ));
            }
            claude::ContentBlock::WebSearchToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::WebFetchToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::AdvisorToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::CodeExecutionToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::BashCodeExecutionToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::TextEditorCodeExecutionToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::ToolSearchToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    server_tool_result_output(&block.content),
                ));
            }
            claude::ContentBlock::McpToolUse(block) => {
                output.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::McpCall {
                        id: block.id,
                        arguments: json_object_to_string(&block.input),
                        name: block.name,
                        server_label: block.server_name,
                        approval_request_id: None,
                        error: None,
                        output: None,
                        status: Some(openai::ResponseMcpCallStatus::Completed),
                        extra: Default::default(),
                    },
                ));
            }
            claude::ContentBlock::McpToolResult(block) => {
                output.push(compact_function_call_output_item(
                    block.tool_use_id,
                    openai::ResponseOutput::Text(response_mcp_tool_result_content_to_text(
                        block.content,
                    )),
                ));
            }
            claude::ContentBlock::Compaction(block) => {
                if let Some(text) = block.content {
                    message_parts.push(openai::CompactMessageContentPart::SummaryText(
                        openai::CompactSummaryTextContent {
                            text,
                            type_: openai::CompactSummaryTextContentType::SummaryText,
                            extra: Default::default(),
                        },
                    ));
                }
                output.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content: block.encrypted_content,
                        id: None,
                        created_by: None,
                        extra: Default::default(),
                    },
                ));
            }
            _ => {}
        }
    }

    if !message_parts.is_empty() {
        output.insert(
            0,
            openai::CompactResponseItem::Message(openai::CompactMessageItem {
                id,
                type_: openai::ResponseMessageItemType::Message,
                content: message_parts,
                role: openai::CompactMessageRole::Assistant,
                status: compact_message_status(stop_reason),
                phase: None,
                extra: Default::default(),
            }),
        );
    }

    output
}

fn compact_message_status(stop_reason: &claude::StopReason) -> openai::ResponseItemLifecycleStatus {
    match stop_reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => {
            openai::ResponseItemLifecycleStatus::Incomplete
        }
        _ => openai::ResponseItemLifecycleStatus::Completed,
    }
}
