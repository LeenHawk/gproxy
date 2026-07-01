use crate::protocol::{claude, openai};

use super::DEFAULT_REASONING_ID;
use super::tools::{
    function_call_output_item, mcp_tool_result_content_to_text, server_tool_result_output,
    server_tool_use_item, tool_result_content_to_openai,
};
use super::util::{
    document_source_to_input_part, image_source_to_input_part, join_text, json_object_to_string,
};

pub(super) fn system_to_openai_item(text: String) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
        openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role: openai::ResponseEasyInputMessageRole::System,
            content: openai::ResponseEasyInputContent::Text(text),
            phase: None,
            extra: Default::default(),
        },
    ))
}

pub(super) fn claude_messages_to_openai_items(
    messages: Vec<claude::MessageParam>,
) -> Vec<openai::ResponseItem> {
    messages
        .into_iter()
        .flat_map(claude_message_to_openai_items)
        .collect()
}

fn claude_message_to_openai_items(message: claude::MessageParam) -> Vec<openai::ResponseItem> {
    let role = claude_role_to_openai(message.role);
    let mut items = Vec::new();
    let mut message_parts = Vec::new();

    match message.content {
        claude::MessageContent::String(text) => {
            if !text.is_empty() {
                message_parts.push(openai::ResponseInputContentPart::InputText {
                    text,
                    extra: Default::default(),
                });
            }
        }
        claude::MessageContent::Array(blocks) => {
            for block in blocks {
                match claude_request_block_to_openai(block) {
                    ClaudeRequestBlockItem::MessagePart(part) => message_parts.push(part),
                    ClaudeRequestBlockItem::Item(item) => items.push(item),
                    ClaudeRequestBlockItem::None => {}
                }
            }
        }
    }

    if !message_parts.is_empty() {
        items.push(openai::ResponseItem::Message(
            openai::ResponseMessageItem::EasyInput(openai::ResponseEasyInputMessageItem {
                type_: Some(openai::ResponseMessageItemType::Message),
                role,
                content: openai::ResponseEasyInputContent::Parts(message_parts),
                phase: None,
                extra: Default::default(),
            }),
        ));
    }

    items
}

pub(super) enum ClaudeRequestBlockItem {
    MessagePart(openai::ResponseInputContentPart),
    Item(openai::ResponseItem),
    None,
}

fn claude_role_to_openai(role: claude::MessageRole) -> openai::ResponseEasyInputMessageRole {
    match role {
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            openai::ResponseEasyInputMessageRole::Assistant
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            openai::ResponseEasyInputMessageRole::System
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::User)
        | claude::MessageRole::Unknown(_) => openai::ResponseEasyInputMessageRole::User,
    }
}

fn claude_request_block_to_openai(block: claude::ContentBlockParam) -> ClaudeRequestBlockItem {
    match block {
        claude::ContentBlockParam::Text(block) => {
            ClaudeRequestBlockItem::MessagePart(openai::ResponseInputContentPart::InputText {
                text: block.text,
                extra: Default::default(),
            })
        }
        claude::ContentBlockParam::Image(block) => image_source_to_input_part(block.source)
            .map(ClaudeRequestBlockItem::MessagePart)
            .unwrap_or(ClaudeRequestBlockItem::None),
        claude::ContentBlockParam::Document(block) => {
            document_source_to_input_part(block.source, block.title)
                .map(ClaudeRequestBlockItem::MessagePart)
                .unwrap_or(ClaudeRequestBlockItem::None)
        }
        claude::ContentBlockParam::ToolUse(block) => ClaudeRequestBlockItem::Item(
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                arguments: json_object_to_string(&block.input),
                call_id: block.id.clone(),
                name: block.name,
                id: Some(block.id),
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                extra: Default::default(),
            }),
        ),
        claude::ContentBlockParam::ToolResult(block) => function_call_output_item(
            block.tool_use_id,
            tool_result_content_to_openai(block.content),
        ),
        claude::ContentBlockParam::Thinking(block) => ClaudeRequestBlockItem::Item(
            openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
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
            }),
        ),
        claude::ContentBlockParam::RedactedThinking(block) => ClaudeRequestBlockItem::Item(
            openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
                id: DEFAULT_REASONING_ID.to_owned(),
                summary: Vec::new(),
                content: None,
                encrypted_content: Some(block.data),
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                extra: Default::default(),
            }),
        ),
        claude::ContentBlockParam::Compaction(block) => {
            let Some(encrypted_content) = block.encrypted_content else {
                return block
                    .content
                    .map(|text| {
                        ClaudeRequestBlockItem::MessagePart(
                            openai::ResponseInputContentPart::InputText {
                                text,
                                extra: Default::default(),
                            },
                        )
                    })
                    .unwrap_or(ClaudeRequestBlockItem::None);
            };
            ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(
                openai::TypedResponseItem::Compaction {
                    encrypted_content,
                    id: None,
                    created_by: None,
                    extra: Default::default(),
                },
            ))
        }
        claude::ContentBlockParam::ServerToolUse(block) => {
            server_tool_use_item(block.id, block.input, block.name)
        }
        claude::ContentBlockParam::WebSearchToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::WebFetchToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::AdvisorToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::CodeExecutionToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::BashCodeExecutionToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::TextEditorCodeExecutionToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::ToolSearchToolResult(block) => {
            function_call_output_item(block.tool_use_id, server_tool_result_output(&block.content))
        }
        claude::ContentBlockParam::McpToolUse(block) => ClaudeRequestBlockItem::Item(
            openai::ResponseItem::Typed(openai::TypedResponseItem::McpCall {
                id: block.id,
                arguments: json_object_to_string(&block.input),
                name: block.name,
                server_label: block.server_name,
                approval_request_id: None,
                error: None,
                output: None,
                status: Some(openai::ResponseMcpCallStatus::Completed),
                extra: Default::default(),
            }),
        ),
        claude::ContentBlockParam::McpToolResult(block) => function_call_output_item(
            block.tool_use_id,
            openai::ResponseOutput::Text(mcp_tool_result_content_to_text(block.content)),
        ),
        claude::ContentBlockParam::MidConversationSystem(block) => {
            let text = join_text(block.content.into_iter().map(|block| block.text));
            if text.is_empty() {
                ClaudeRequestBlockItem::None
            } else {
                ClaudeRequestBlockItem::Item(system_to_openai_item(text))
            }
        }
        _ => ClaudeRequestBlockItem::None,
    }
}
