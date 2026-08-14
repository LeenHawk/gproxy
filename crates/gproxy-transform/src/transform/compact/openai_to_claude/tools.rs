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
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolUse(crate::protocol::wire!(
                claude::ToolUseBlock {
                    id: normalized_tool_id(call_id),
                    input: arguments_to_json_object(&arguments),
                    name,
                    type_: claude::ToolUseBlockType::ToolUse,
                    cache_control: None,
                    caller: None,
                }
            ))],
        ),
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolUse(crate::protocol::wire!(
                claude::ToolUseBlock {
                    id: normalized_tool_id(call_id),
                    input: string_input_json_object(input),
                    name,
                    type_: claude::ToolUseBlockType::ToolUse,
                    cache_control: None,
                    caller: None,
                }
            ))],
        ),
        openai::TypedResponseItem::ApplyPatchCall {
            call_id, operation, ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolUse(crate::protocol::wire!(
                claude::ToolUseBlock {
                    id: normalized_tool_id(call_id),
                    input: apply_patch_to_text_editor_input(operation),
                    name: "str_replace_based_edit_tool".to_owned(),
                    type_: claude::ToolUseBlockType::ToolUse,
                    cache_control: None,
                    caller: None,
                }
            ))],
        ),
        openai::TypedResponseItem::WebSearchCall { id, action, .. } => {
            let (name, input) = web_action_to_claude(action);
            (
                claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
                vec![claude::ContentBlockParam::ServerToolUse(
                    server_tool_use_block(id, input, name),
                )],
            )
        }
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
            vec![bash_tool_use_block(
                call_id,
                local_shell_to_bash_input(action),
            )],
        ),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: None,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![bash_tool_use_block(
                call_id,
                shell_to_bash_input(action, None),
            )],
        ),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: Some(environment),
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![bash_tool_use_block(
                call_id,
                shell_to_bash_input(action, Some(environment)),
            )],
        ),
        openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ServerToolUse(
                server_tool_use_block(
                    id.or(call_id).unwrap_or_else(|| "tool_search".to_owned()),
                    value_to_json_object(arguments),
                    if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
                        claude::ServerToolUseNameKnown::ToolSearchToolRegex
                    } else {
                        claude::ServerToolUseNameKnown::ToolSearchToolBm25
                    },
                ),
            )],
        ),
        openai::TypedResponseItem::ToolSearchOutput {
            tools,
            id,
            call_id,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant),
            vec![claude::ContentBlockParam::ToolSearchToolResult(
                crate::protocol::wire!(claude::ToolSearchToolResultBlock {
                    content: claude::ToolSearchToolResultContent::Result(
                        crate::protocol::wire!(claude::ToolSearchToolSearchResultBlock {
                            tool_references: response_tools_to_references(tools),
                            type_: claude::ToolSearchToolSearchResultBlockType::ToolSearchToolSearchResult,
                            extra: Default::default(),
                        }),
                    ),
                    tool_use_id: call_id.or(id).unwrap_or_else(|| "tool_search".to_owned()),
                    type_: claude::ToolSearchToolResultBlockType::ToolSearchToolResult,
                    cache_control: None,
                    extra: Default::default(),
                }),
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
                crate::protocol::wire!(claude::ToolResultBlock {
                    tool_use_id: normalized_tool_id(call_id),
                    type_: claude::ToolResultBlockType::ToolResult,
                    cache_control: None,
                    content: response_output_to_tool_result(output),
                    is_error: None,
                }),
            )],
        ),
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            output,
            status,
            ..
        } => (
            claude::MessageRole::Known(claude::MessageRoleKnown::User),
            vec![claude::ContentBlockParam::ToolResult(
                crate::protocol::wire!(claude::ToolResultBlock {
                    tool_use_id: normalized_tool_id(call_id),
                    type_: claude::ToolResultBlockType::ToolResult,
                    cache_control: None,
                    content: output.map(claude::ToolResultContent::Text),
                    is_error: Some(matches!(
                        status,
                        openai::ResponseApplyPatchCallOutputStatus::Failed
                    )),
                }),
            )],
        ),
        openai::TypedResponseItem::LocalShellCallOutput { id, output, .. } => {
            tool_result_message(id, output, false)
        }
        openai::TypedResponseItem::ShellCallOutput {
            call_id, output, ..
        } => {
            let is_error = output.iter().any(|part| match part.outcome {
                openai::ShellCallOutcome::Exit { exit_code } => exit_code != 0,
                openai::ShellCallOutcome::Timeout {} => true,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            });
            let text = output
                .into_iter()
                .flat_map(|part| [part.stdout, part.stderr])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            tool_result_message(call_id, text, is_error)
        }
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
                crate::protocol::wire!(claude::McpToolUseBlock {
                    id: id.clone(),
                    input: arguments_to_json_object(&arguments),
                    name,
                    server_name: server_label,
                    type_: claude::McpToolUseBlockType::McpToolUse,
                    cache_control: None,
                }),
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
            let thinking = content
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .collect::<Vec<_>>()
                .join("");
            if !thinking.is_empty() {
                if let Some(signature) = encrypted_content.as_ref() {
                    blocks.push(claude::ContentBlockParam::Thinking(crate::protocol::wire!(
                        claude::ThinkingBlock {
                            signature: signature.clone(),
                            thinking: thinking.clone(),
                            type_: claude::ThinkingBlockType::Thinking,
                        }
                    )));
                } else {
                    blocks.extend(text_block(thinking.clone()));
                }
            } else if let Some(encrypted_content) = encrypted_content {
                blocks.push(claude::ContentBlockParam::RedactedThinking(
                    crate::protocol::wire!(claude::RedactedThinkingBlock {
                        data: encrypted_content,
                        type_: claude::RedactedThinkingBlockType::RedactedThinking,
                    }),
                ));
            }
            blocks.extend(summary.into_iter().filter_map(|part| text_block(part.text)));
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
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn normalized_tool_id(value: String) -> String {
    if value.starts_with("toolu_") {
        value
    } else {
        format!("toolu_{value}")
    }
}

fn tool_output_part_to_claude(
    part: openai::ResponseToolOutputContentPart,
) -> Option<claude::ToolResultContentBlock> {
    match part {
        openai::ResponseToolOutputContentPart::InputText { text, .. } => Some(
            claude::ToolResultContentBlock::Text(crate::protocol::wire!(claude::TextBlock {
                text,
                type_: claude::TextBlockType::Text,
                cache_control: None,
                citations: None,
                extra: Default::default(),
            })),
        ),
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
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn server_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> claude::ServerToolUseBlock {
    crate::protocol::wire!(claude::ServerToolUseBlock {
        id,
        input,
        name: claude::ServerToolUseName::Known(name),
        type_: claude::ServerToolUseBlockType::ServerToolUse,
        cache_control: None,
        caller: None,
    })
}

pub(super) fn response_server_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> claude::ResponseServerToolUseBlock {
    crate::protocol::wire!(claude::ResponseServerToolUseBlock {
        id,
        input,
        name: claude::ServerToolUseName::Known(name),
        type_: claude::ServerToolUseBlockType::ServerToolUse,
        caller: None,
        extra: Default::default(),
    })
}

fn mcp_result_block(
    tool_use_id: String,
    output: Option<String>,
    error: Option<String>,
) -> Option<claude::McpToolResultBlock> {
    let is_error = error.is_some();
    let content = error.or(output)?;
    Some(crate::protocol::wire!(claude::McpToolResultBlock {
        tool_use_id,
        type_: claude::McpToolResultBlockType::McpToolResult,
        cache_control: None,
        content: Some(claude::McpToolResultContent::String(content)),
        is_error: Some(is_error),
    }))
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

fn bash_tool_use_block(id: String, input: claude::JsonObject) -> claude::ContentBlockParam {
    claude::ContentBlockParam::ToolUse(crate::protocol::wire!(claude::ToolUseBlock {
        id: normalized_tool_id(id),
        input,
        name: "bash".to_owned(),
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
    }))
}

fn tool_result_message(
    call_id: String,
    output: String,
    is_error: bool,
) -> (claude::MessageRole, Vec<claude::ContentBlockParam>) {
    (
        claude::MessageRole::Known(claude::MessageRoleKnown::User),
        vec![claude::ContentBlockParam::ToolResult(
            crate::protocol::wire!(claude::ToolResultBlock {
                tool_use_id: normalized_tool_id(call_id),
                type_: claude::ToolResultBlockType::ToolResult,
                cache_control: None,
                content: (!output.is_empty()).then_some(claude::ToolResultContent::Text(output)),
                is_error: Some(is_error),
            }),
        )],
    )
}

pub(crate) fn web_action_to_claude(
    action: openai::WebSearchAction,
) -> (claude::ServerToolUseNameKnown, claude::JsonObject) {
    match action {
        openai::WebSearchAction::OpenPage { url } => {
            let mut input = claude::JsonObject::new();
            if let Some(url) = url {
                input.insert("url".to_owned(), serde_json::Value::String(url));
            }
            (claude::ServerToolUseNameKnown::WebFetch, input)
        }
        action => (
            claude::ServerToolUseNameKnown::WebSearch,
            serializable_to_json_object(&action),
        ),
    }
}

pub(crate) fn local_shell_to_bash_input(action: openai::LocalShellAction) -> claude::JsonObject {
    let mut input = claude::JsonObject::new();
    input.insert(
        "command".to_owned(),
        serde_json::Value::String(action.command.join("\n")),
    );
    if !action.env.is_empty() {
        input.insert(
            "env".to_owned(),
            serde_json::to_value(action.env).unwrap_or_default(),
        );
    }
    if let Some(timeout_ms) = action.timeout_ms {
        input.insert("timeout_ms".to_owned(), timeout_ms.into());
    }
    if let Some(user) = action.user {
        input.insert("user".to_owned(), user.into());
    }
    if let Some(directory) = action.working_directory {
        input.insert("working_directory".to_owned(), directory.into());
    }
    input
}

pub(crate) fn shell_to_bash_input(
    action: openai::ShellAction,
    environment: Option<openai::ShellEnvironment>,
) -> claude::JsonObject {
    let mut input = claude::JsonObject::new();
    input.insert(
        "command".to_owned(),
        serde_json::Value::String(action.commands.join("\n")),
    );
    if let Some(timeout_ms) = action.timeout_ms {
        input.insert("timeout_ms".to_owned(), timeout_ms.into());
    }
    if let Some(max_output_length) = action.max_output_length {
        input.insert("max_output_length".to_owned(), max_output_length.into());
    }
    if let Some(environment) = environment {
        input.insert(
            "environment".to_owned(),
            serde_json::to_value(environment).unwrap_or_default(),
        );
    }
    input
}

pub(crate) fn apply_patch_to_text_editor_input(
    operation: openai::ApplyPatchOperation,
) -> claude::JsonObject {
    let mut input = claude::JsonObject::new();
    match operation {
        openai::ApplyPatchOperation::CreateFile { diff, path } => {
            input.insert("command".to_owned(), "create".into());
            input.insert("path".to_owned(), path.into());
            input.insert("file_text".to_owned(), diff.into());
        }
        openai::ApplyPatchOperation::DeleteFile { path } => {
            input.insert("command".to_owned(), "delete".into());
            input.insert("path".to_owned(), path.into());
        }
        openai::ApplyPatchOperation::UpdateFile { diff, path } => {
            let (old_str, new_str) = replacement_strings_from_diff(&diff);
            input.insert("command".to_owned(), "str_replace".into());
            input.insert("path".to_owned(), path.into());
            input.insert("old_str".to_owned(), old_str.into());
            input.insert("new_str".to_owned(), new_str.into());
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
    input
}

fn replacement_strings_from_diff(diff: &str) -> (String, String) {
    let mut old = String::new();
    let mut new = String::new();
    for line in diff.lines() {
        if line.starts_with("@@")
            || line.starts_with("***")
            || line.starts_with("---")
            || line.starts_with("+++")
        {
            continue;
        }
        if let Some(line) = line.strip_prefix('-') {
            old.push_str(line);
            old.push('\n');
        } else if let Some(line) = line.strip_prefix('+') {
            new.push_str(line);
            new.push('\n');
        } else {
            let line = line.strip_prefix(' ').unwrap_or(line);
            old.push_str(line);
            old.push('\n');
            new.push_str(line);
            new.push('\n');
        }
    }
    (old, new)
}

fn response_tools_to_references(
    tools: Vec<openai::ResponseTool>,
) -> Vec<claude::ToolReferenceBlock> {
    tools
        .into_iter()
        .filter_map(|tool| match tool {
            openai::ResponseTool::Function { name, .. }
            | openai::ResponseTool::Custom { name, .. } => Some(name),
            _ => None,
        })
        .map(|tool_name| {
            crate::protocol::wire!(claude::ToolReferenceBlock {
                tool_name,
                type_: claude::ToolReferenceBlockType::ToolReference,
                cache_control: None,
            })
        })
        .collect()
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
