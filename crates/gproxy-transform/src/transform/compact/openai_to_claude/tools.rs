use crate::protocol::{claude, openai};

use super::input::{blocks_to_claude_message, document_block, image_block, text_block};

pub(super) fn typed_item_to_claude_message(
    item: openai::TypedResponseItem,
) -> Option<claude::MessageParam> {
    let (role, blocks) = match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
                id: id.unwrap_or(call_id),
                input: arguments_to_json_object(&arguments),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                cache_control: None,
                caller: None,
            })],
        ),
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
                id: id.unwrap_or(call_id),
                input: string_input_json_object(input),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                cache_control: None,
                caller: None,
            })],
        ),
        openai::TypedResponseItem::WebSearchCall { id, action, .. } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    id,
                    serializable_to_json_object(&action),
                    claude::ServerToolUseNameKnown::WebSearch,
                ),
            )],
        ),
        openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    id,
                    code_interpreter_input(code, container_id),
                    claude::ServerToolUseNameKnown::CodeExecution,
                ),
            )],
        ),
        openai::TypedResponseItem::LocalShellCall {
            action, call_id, ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    call_id,
                    serializable_to_json_object(&action),
                    claude::ServerToolUseNameKnown::BashCodeExecution,
                ),
            )],
        ),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: None,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    call_id,
                    serializable_to_json_object(&action),
                    claude::ServerToolUseNameKnown::BashCodeExecution,
                ),
            )],
        ),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: Some(environment),
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    call_id,
                    shell_input(action, environment),
                    claude::ServerToolUseNameKnown::BashCodeExecution,
                ),
            )],
        ),
        openai::TypedResponseItem::FunctionCallOutput {
            call_id, output, ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::User),
            vec![claude::ContentBlockParam::ToolResult(
                claude::ToolResultBlock {
                    tool_use_id: call_id,
                    type_: claude::ToolResultBlockType::ToolResult,
                    cache_control: None,
                    content: response_output_to_tool_result(output),
                    is_error: None,
                },
            )],
        ),
        openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            output,
            error,
            ..
        } => {
            let mut blocks = vec![claude::ContentBlockParam::McpToolUse(
                claude::McpToolUseBlock {
                    id: id.clone(),
                    input: arguments_to_json_object(&arguments),
                    name,
                    server_name: server_label,
                    type_: claude::McpToolUseBlockType::McpToolUse,
                    cache_control: None,
                },
            )];
            if let Some(result) = mcp_result_block(id, output, error) {
                blocks.push(claude::ContentBlockParam::McpToolResult(result));
            }
            (
                claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                blocks,
            )
        }
        openai::TypedResponseItem::Reasoning {
            id: _,
            summary,
            content,
            encrypted_content,
            ..
        } => {
            let mut blocks = Vec::new();
            if let Some(encrypted_content) = encrypted_content {
                blocks.push(claude::ContentBlockParam::RedactedThinking(
                    claude::RedactedThinkingBlock {
                        data: encrypted_content,
                        type_: claude::RedactedThinkingBlockType::RedactedThinking,
                    },
                ));
            }
            blocks.extend(summary.into_iter().filter_map(|part| text_block(part.text)));
            blocks.extend(
                content
                    .into_iter()
                    .flatten()
                    .filter_map(|part| text_block(part.text)),
            );
            (
                claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                blocks,
            )
        }
        _ => return None,
    };

    blocks_to_claude_message(role, blocks)
}

fn response_output_to_tool_result(
    output: openai::ResponseOutput,
) -> Option<claude::ToolResultContent> {
    match output {
        openai::ResponseOutput::Text(text) => {
            (!text.is_empty()).then_some(claude::ToolResultContent::Text(text))
        }
        openai::ResponseOutput::Parts(parts) => {
            let blocks = parts
                .into_iter()
                .filter_map(tool_output_part_to_claude)
                .collect::<Vec<_>>();
            (!blocks.is_empty()).then_some(claude::ToolResultContent::Blocks(blocks))
        }
    }
}

fn tool_output_part_to_claude(
    part: openai::ResponseToolOutputContentPart,
) -> Option<claude::ToolResultContentBlock> {
    match part {
        openai::ResponseToolOutputContentPart::InputText { text, .. } => {
            Some(claude::ToolResultContentBlock::Text(claude::TextBlock {
                text,
                type_: claude::TextBlockType::Text,
                cache_control: None,
                citations: None,
                extra: Default::default(),
            }))
        }
        openai::ResponseToolOutputContentPart::InputImage {
            file_id, image_url, ..
        } => image_block(file_id, image_url).and_then(|block| match block {
            claude::ContentBlockParam::Image(block) => {
                Some(claude::ToolResultContentBlock::Image(block))
            }
            _ => None,
        }),
        openai::ResponseToolOutputContentPart::InputFile {
            file_data,
            file_id,
            file_url,
            filename,
            ..
        } => document_block(file_id, file_url, file_data, filename).and_then(|block| match block {
            claude::ContentBlockParam::Document(block) => {
                Some(claude::ToolResultContentBlock::Document(block))
            }
            _ => None,
        }),
    }
}

fn server_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> claude::ServerToolUseBlock {
    claude::ServerToolUseBlock {
        id,
        input,
        name: claude::ServerToolUseName::Known(name),
        type_: claude::ServerToolUseBlockType::ServerToolUse,
        cache_control: None,
        caller: None,
    }
}

pub(super) fn response_server_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> claude::ResponseServerToolUseBlock {
    claude::ResponseServerToolUseBlock {
        id,
        input,
        name: claude::ServerToolUseName::Known(name),
        type_: claude::ServerToolUseBlockType::ServerToolUse,
        caller: None,
        extra: Default::default(),
    }
}

fn mcp_result_block(
    tool_use_id: String,
    output: Option<String>,
    error: Option<String>,
) -> Option<claude::McpToolResultBlock> {
    let is_error = error.is_some();
    let content = error.or(output)?;
    Some(claude::McpToolResultBlock {
        tool_use_id,
        type_: claude::McpToolResultBlockType::McpToolResult,
        cache_control: None,
        content: Some(claude::McpToolResultContent::String(content)),
        is_error: Some(is_error),
    })
}

pub(super) fn arguments_to_json_object(arguments: &str) -> claude::JsonObject {
    serde_json::from_str(arguments)
        .map(value_to_json_object)
        .unwrap_or_else(|_| string_input_json_object(arguments.to_owned()))
}

pub(super) fn string_input_json_object(input: String) -> claude::JsonObject {
    let mut object = claude::JsonObject::new();
    object.insert("input".to_owned(), serde_json::Value::String(input));
    object
}

fn value_to_json_object(value: serde_json::Value) -> claude::JsonObject {
    match value {
        serde_json::Value::Object(map) => map.into_iter().collect(),
        value => {
            let mut object = claude::JsonObject::new();
            object.insert("value".to_owned(), value);
            object
        }
    }
}

pub(super) fn serializable_to_json_object<T: serde::Serialize>(value: &T) -> claude::JsonObject {
    serde_json::to_value(value)
        .map(value_to_json_object)
        .unwrap_or_default()
}

pub(super) fn code_interpreter_input(
    code: Option<String>,
    container_id: String,
) -> claude::JsonObject {
    let mut input = claude::JsonObject::new();
    if let Some(code) = code {
        input.insert("code".to_owned(), serde_json::Value::String(code));
    }
    input.insert(
        "container_id".to_owned(),
        serde_json::Value::String(container_id),
    );
    input
}

pub(super) fn shell_input(
    action: openai::ShellAction,
    environment: openai::ShellEnvironment,
) -> claude::JsonObject {
    let mut input = serializable_to_json_object(&action);
    input.insert(
        "environment".to_owned(),
        serde_json::to_value(environment).unwrap_or(serde_json::Value::Null),
    );
    input
}
