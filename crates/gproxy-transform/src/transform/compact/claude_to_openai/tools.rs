use crate::protocol::{claude, openai};

use super::input::ClaudeRequestBlockItem;
use super::util::{
    document_source_to_input_part, image_source_to_input_part, join_text, json_object_to_string,
    server_tool_name_to_string,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApproximateToolKind {
    Shell,
    ApplyPatch,
}

pub(super) fn tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: String,
) -> (ClaudeRequestBlockItem, Option<ApproximateToolKind>) {
    let (item, kind) = typed_tool_call(id, input, name);
    (
        ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(item)),
        kind,
    )
}

pub(super) fn compact_tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: String,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(typed_tool_call(id, input, name).0)
}

pub(crate) fn typed_tool_call(
    id: String,
    input: claude::JsonObject,
    name: String,
) -> (openai::TypedResponseItem, Option<ApproximateToolKind>) {
    if name == "bash"
        && let Some(action) = shell_action_from_claude(&input)
    {
        return (
            openai::TypedResponseItem::ShellCall {
                action,
                call_id: id.clone(),
                id: Some(id),
                caller: None,
                environment: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                extra: Default::default(),
            },
            Some(ApproximateToolKind::Shell),
        );
    }
    if matches!(
        name.as_str(),
        "str_replace_editor" | "str_replace_based_edit_tool"
    ) && let Some(operation) = apply_patch_operation_from_claude(&input)
    {
        return (
            openai::TypedResponseItem::ApplyPatchCall {
                call_id: id.clone(),
                operation,
                status: openai::ResponseApplyPatchCallStatus::Completed,
                id: Some(id),
                caller: None,
                created_by: None,
                extra: Default::default(),
            },
            Some(ApproximateToolKind::ApplyPatch),
        );
    }
    (
        crate::protocol::wire!(openai::TypedResponseItem::FunctionCall {
            arguments: json_object_to_string(&input),
            call_id: id.clone(),
            name,
            id: Some(id),
            caller: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            extra: Default::default(),
        }),
        None,
    )
}

pub(super) fn server_tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseName,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(server_tool_call(
        id, input, name,
    )))
}

pub(super) fn function_call_output_item(
    call_id: String,
    output: openai::ResponseOutput,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(
        openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            id: None,
            caller: None,
            name: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            extra: Default::default(),
        },
    ))
}

pub(super) fn approximate_tool_result_item(
    kind: ApproximateToolKind,
    call_id: String,
    content: Option<claude::ToolResultContent>,
    is_error: Option<bool>,
) -> ClaudeRequestBlockItem {
    let output = tool_result_content_to_openai(content);
    let text = match output {
        openai::ResponseOutput::Text(text) => text,
        openai::ResponseOutput::Parts(parts) => {
            serde_json::to_string(&parts).unwrap_or_else(|_| String::new())
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    let item = match kind {
        ApproximateToolKind::Shell => openai::TypedResponseItem::ShellCallOutput {
            call_id,
            output: vec![crate::protocol::wire!(openai::ShellCallOutputContent {
                outcome: openai::ShellCallOutcome::Exit {
                    exit_code: if is_error.unwrap_or(false) { 1 } else { 0 },
                },
                stderr: if is_error.unwrap_or(false) {
                    text.clone()
                } else {
                    String::new()
                },
                stdout: if is_error.unwrap_or(false) {
                    String::new()
                } else {
                    text
                },
                created_by: None,
                extra: Default::default(),
            })],
            id: None,
            caller: None,
            max_output_length: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            extra: Default::default(),
        },
        ApproximateToolKind::ApplyPatch => openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            status: if is_error.unwrap_or(false) {
                openai::ResponseApplyPatchCallOutputStatus::Failed
            } else {
                openai::ResponseApplyPatchCallOutputStatus::Completed
            },
            id: None,
            caller: None,
            output: (!text.is_empty()).then_some(text),
            created_by: None,
            extra: Default::default(),
        },
    };
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(item))
}

pub(super) fn compact_server_tool_use_item(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseName,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(server_tool_call(id, input, name))
}

pub(crate) fn server_tool_call(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseName,
) -> openai::TypedResponseItem {
    match name {
        claude::ServerToolUseName::Known(claude::ServerToolUseNameKnown::WebFetch) => {
            openai::TypedResponseItem::WebSearchCall {
                id,
                action: openai::WebSearchAction::OpenPage {
                    url: string_field(&input, "url"),
                },
                status: openai::ResponseWebSearchCallStatus::Completed,
                extra: Default::default(),
            }
        }
        claude::ServerToolUseName::Known(claude::ServerToolUseNameKnown::WebSearch) => {
            openai::TypedResponseItem::WebSearchCall {
                id,
                action: web_search_action_from_claude(&input),
                status: openai::ResponseWebSearchCallStatus::Completed,
                extra: Default::default(),
            }
        }
        claude::ServerToolUseName::Known(claude::ServerToolUseNameKnown::BashCodeExecution) => {
            if let Some(action) = shell_action_from_claude(&input) {
                return openai::TypedResponseItem::ShellCall {
                    action,
                    call_id: id.clone(),
                    id: Some(id),
                    caller: None,
                    environment: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    created_by: None,
                    extra: Default::default(),
                };
            }
            generic_server_tool_call(id, input, claude::ServerToolUseNameKnown::BashCodeExecution)
        }
        claude::ServerToolUseName::Known(
            claude::ServerToolUseNameKnown::TextEditorCodeExecution,
        ) => {
            if let Some(operation) = apply_patch_operation_from_claude(&input) {
                return openai::TypedResponseItem::ApplyPatchCall {
                    call_id: id.clone(),
                    operation,
                    status: openai::ResponseApplyPatchCallStatus::Completed,
                    id: Some(id),
                    caller: None,
                    created_by: None,
                    extra: Default::default(),
                };
            }
            generic_server_tool_call(
                id,
                input,
                claude::ServerToolUseNameKnown::TextEditorCodeExecution,
            )
        }
        claude::ServerToolUseName::Known(
            kind @ (claude::ServerToolUseNameKnown::ToolSearchToolBm25
            | claude::ServerToolUseNameKnown::ToolSearchToolRegex),
        ) => openai::TypedResponseItem::ToolSearchCall {
            arguments: serde_json::Value::Object(input.into_iter().collect()),
            id: Some(id.clone()),
            call_id: Some(id),
            execution: Some(
                if matches!(kind, claude::ServerToolUseNameKnown::ToolSearchToolRegex) {
                    openai::ToolSearchExecution::Client
                } else {
                    openai::ToolSearchExecution::Server
                },
            ),
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            extra: Default::default(),
        },
        known @ claude::ServerToolUseName::Known(_) => {
            let known_name = match known {
                claude::ServerToolUseName::Known(value) => value,
                _ => unreachable!(),
            };
            generic_server_tool_call(id, input, known_name)
        }
        unknown => crate::protocol::wire!(openai::TypedResponseItem::FunctionCall {
            arguments: json_object_to_string(&input),
            call_id: id.clone(),
            name: server_tool_name_to_string(&unknown),
            id: Some(id),
            caller: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            extra: Default::default(),
        }),
    }
}

fn generic_server_tool_call(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> openai::TypedResponseItem {
    crate::protocol::wire!(openai::TypedResponseItem::FunctionCall {
        arguments: json_object_to_string(&input),
        call_id: id.clone(),
        name: server_tool_name_to_string(&claude::ServerToolUseName::Known(name)),
        id: Some(id),
        caller: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}

fn web_search_action_from_claude(input: &claude::JsonObject) -> openai::WebSearchAction {
    if let Some(url) = string_field(input, "url")
        && let Some(pattern) = string_field(input, "pattern")
    {
        return openai::WebSearchAction::FindInPage { pattern, url };
    }
    openai::WebSearchAction::Search {
        queries: string_array_field(input, "queries"),
        query: string_field(input, "query"),
        sources: None,
    }
}

fn shell_action_from_claude(input: &claude::JsonObject) -> Option<openai::ShellAction> {
    let commands = string_array_or_string_field(input, "commands")
        .or_else(|| string_array_or_string_field(input, "command"))?;
    Some(crate::protocol::wire!(openai::ShellAction {
        commands,
        max_output_length: u32_field(input, "max_output_length"),
        timeout_ms: u32_field(input, "timeout_ms").or_else(|| u32_field(input, "timeout")),
        extra: Default::default(),
    }))
}

fn apply_patch_operation_from_claude(
    input: &claude::JsonObject,
) -> Option<openai::ApplyPatchOperation> {
    let command = string_field(input, "command")?;
    let path = string_field(input, "path")?;
    match command.as_str() {
        "create" => Some(openai::ApplyPatchOperation::CreateFile {
            diff: string_field(input, "file_text").unwrap_or_default(),
            path,
        }),
        "str_replace" => Some(openai::ApplyPatchOperation::UpdateFile {
            diff: replacement_diff(
                &string_field(input, "old_str").unwrap_or_default(),
                &string_field(input, "new_str").unwrap_or_default(),
            ),
            path,
        }),
        _ => None,
    }
}

fn replacement_diff(old: &str, new: &str) -> String {
    let mut diff = String::from("@@\n");
    for line in old.split_inclusive('\n') {
        diff.push('-');
        diff.push_str(line);
    }
    if !old.is_empty() && !old.ends_with('\n') {
        diff.push('\n');
    }
    for line in new.split_inclusive('\n') {
        diff.push('+');
        diff.push_str(line);
    }
    diff
}

fn string_field(input: &claude::JsonObject, name: &str) -> Option<String> {
    input.get(name)?.as_str().map(ToOwned::to_owned)
}

fn string_array_field(input: &claude::JsonObject, name: &str) -> Option<Vec<String>> {
    let values = input.get(name)?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
    )
}

fn string_array_or_string_field(input: &claude::JsonObject, name: &str) -> Option<Vec<String>> {
    string_array_field(input, name).or_else(|| string_field(input, name).map(|value| vec![value]))
}

fn u32_field(input: &claude::JsonObject, name: &str) -> Option<u32> {
    input
        .get(name)?
        .as_u64()
        .map(|value| u32::try_from(value).unwrap_or(u32::MAX))
}

pub(super) fn compact_function_call_output_item(
    call_id: String,
    output: openai::ResponseOutput,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCallOutput {
        call_id,
        output,
        id: None,
        caller: None,
        name: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        extra: Default::default(),
    })
}

pub(super) fn tool_result_content_to_openai(
    content: Option<claude::ToolResultContent>,
) -> openai::ResponseOutput {
    match content {
        Some(claude::ToolResultContent::Text(text)) => openai::ResponseOutput::Text(text),
        Some(claude::ToolResultContent::Blocks(blocks)) => {
            let parts = blocks
                .into_iter()
                .filter_map(tool_result_block_to_openai)
                .collect::<Vec<_>>();
            openai::ResponseOutput::Parts(parts)
        }
        Some(claude::ToolResultContent::Raw(value)) => {
            openai::ResponseOutput::Text(value.to_string())
        }
        None => openai::ResponseOutput::Text(String::new()),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(super) fn server_tool_result_output<T: serde::Serialize>(
    content: &T,
) -> openai::ResponseOutput {
    openai::ResponseOutput::Text(serde_json::to_string(content).unwrap_or_else(|_| String::new()))
}

pub(super) fn shell_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(shell_result(call_id, content)))
}

pub(super) fn compact_shell_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(shell_result(call_id, content))
}

pub(crate) fn shell_result<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::TypedResponseItem {
    let value = serde_json::to_value(content).unwrap_or_default();
    let stdout = string_value_field(&value, "stdout");
    let stderr = string_value_field(&value, "stderr")
        .or_else(|| string_value_field(&value, "error_code"))
        .unwrap_or_default();
    let return_code = value
        .get("return_code")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(if stderr.is_empty() { 0 } else { 1 });
    openai::TypedResponseItem::ShellCallOutput {
        call_id,
        output: vec![crate::protocol::wire!(openai::ShellCallOutputContent {
            outcome: openai::ShellCallOutcome::Exit {
                exit_code: return_code,
            },
            stderr,
            stdout: stdout.unwrap_or_default(),
            created_by: None,
            extra: Default::default(),
        })],
        id: None,
        caller: None,
        max_output_length: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        extra: Default::default(),
    }
}

pub(super) fn apply_patch_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(apply_patch_result(
        call_id, content,
    )))
}

pub(super) fn compact_apply_patch_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(apply_patch_result(call_id, content))
}

pub(crate) fn apply_patch_result<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::TypedResponseItem {
    let value = serde_json::to_value(content).unwrap_or_default();
    openai::TypedResponseItem::ApplyPatchCallOutput {
        call_id,
        status: if value.get("error_code").is_some() {
            openai::ResponseApplyPatchCallOutputStatus::Failed
        } else {
            openai::ResponseApplyPatchCallOutputStatus::Completed
        },
        id: None,
        caller: None,
        output: serde_json::to_string(&value).ok(),
        created_by: None,
        extra: Default::default(),
    }
}

pub(super) fn tool_search_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> ClaudeRequestBlockItem {
    ClaudeRequestBlockItem::Item(openai::ResponseItem::Typed(tool_search_result(
        call_id, content,
    )))
}

pub(super) fn compact_tool_search_result_item<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(tool_search_result(call_id, content))
}

pub(crate) fn tool_search_result<T: serde::Serialize>(
    call_id: String,
    content: &T,
) -> openai::TypedResponseItem {
    let value = serde_json::to_value(content).unwrap_or_default();
    let tools = value
        .get("tool_references")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.get("tool_name")?.as_str())
        .map(|name| openai::ResponseTool::Function {
            name: name.to_owned(),
            parameters: Default::default(),
            strict: None,
            defer_loading: None,
            description: None,
            allowed_callers: None,
            extra: Default::default(),
        })
        .collect();
    openai::TypedResponseItem::ToolSearchOutput {
        tools,
        id: None,
        call_id: Some(call_id),
        execution: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        extra: Default::default(),
    }
}

fn string_value_field(value: &serde_json::Value, name: &str) -> Option<String> {
    value.get(name)?.as_str().map(ToOwned::to_owned)
}

pub(crate) fn prepare_response_output_item(item: &mut openai::TypedResponseItem) {
    match item {
        openai::TypedResponseItem::ShellCall { environment, .. } => {
            if environment.is_none() {
                *environment = Some(openai::ShellEnvironment::Local { skills: None });
            }
        }
        openai::TypedResponseItem::ShellCallOutput {
            call_id,
            output,
            id,
            max_output_length,
            ..
        } => {
            if id.is_none() {
                *id = Some(format!("{call_id}_output"));
            }
            if max_output_length.is_none() {
                let length = output
                    .iter()
                    .map(|part| part.stdout.len().saturating_add(part.stderr.len()))
                    .sum::<usize>();
                *max_output_length = Some(u32::try_from(length).unwrap_or(u32::MAX));
            }
        }
        openai::TypedResponseItem::ToolSearchOutput {
            call_id,
            id,
            execution,
            ..
        } => {
            let fallback = call_id.clone().unwrap_or_else(|| "tool_search".to_owned());
            if call_id.is_none() {
                *call_id = Some(fallback.clone());
            }
            if id.is_none() {
                *id = Some(format!("{fallback}_output"));
            }
            if execution.is_none() {
                *execution = Some(openai::ToolSearchExecution::Server);
            }
        }
        openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            id,
            status,
            ..
        } => {
            if id.is_none() {
                *id = Some(format!("{call_id}_output"));
            }
            if status.is_none() {
                *status = Some(openai::ResponseItemLifecycleStatus::Completed);
            }
        }
        _ => {}
    }
}

pub(super) fn mcp_tool_result_content_to_text(
    content: Option<claude::McpToolResultContent>,
) -> String {
    match content {
        Some(claude::McpToolResultContent::String(text)) => text,
        Some(claude::McpToolResultContent::Array(blocks)) => {
            join_text(blocks.into_iter().map(|block| block.text))
        }
        None => String::new(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(super) fn response_mcp_tool_result_content_to_text(
    content: claude::ResponseMcpToolResultContent,
) -> String {
    match content {
        claude::ResponseMcpToolResultContent::String(text) => text,
        claude::ResponseMcpToolResultContent::Array(blocks) => {
            join_text(blocks.into_iter().map(|block| block.text))
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn tool_result_block_to_openai(
    block: claude::ToolResultContentBlock,
) -> Option<openai::ResponseToolOutputContentPart> {
    match block {
        claude::ToolResultContentBlock::Text(block) => {
            Some(openai::ResponseToolOutputContentPart::InputText {
                text: block.text,
                prompt_cache_breakpoint: None,
                extra: Default::default(),
            })
        }
        claude::ToolResultContentBlock::Image(block) => {
            input_part_to_tool_output_part(image_source_to_input_part(block.source)?)
        }
        claude::ToolResultContentBlock::Document(block) => input_part_to_tool_output_part(
            document_source_to_input_part(block.source, block.title)?,
        ),
        claude::ToolResultContentBlock::SearchResult(block) => {
            let text = join_text(
                block
                    .content
                    .into_iter()
                    .map(|content_block| content_block.text)
                    .chain([block.source, block.title]),
            );
            (!text.is_empty()).then_some(openai::ResponseToolOutputContentPart::InputText {
                text,
                prompt_cache_breakpoint: None,
                extra: Default::default(),
            })
        }
        claude::ToolResultContentBlock::ToolReference(_)
        | claude::ToolResultContentBlock::Raw(_) => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn input_part_to_tool_output_part(
    part: openai::ResponseInputContentPart,
) -> Option<openai::ResponseToolOutputContentPart> {
    match part {
        openai::ResponseInputContentPart::InputText { text, .. } => {
            Some(openai::ResponseToolOutputContentPart::InputText {
                text,
                prompt_cache_breakpoint: None,
                extra: Default::default(),
            })
        }
        openai::ResponseInputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            ..
        } => Some(openai::ResponseToolOutputContentPart::InputImage {
            detail,
            file_id,
            image_url,
            prompt_cache_breakpoint: None,
            extra: Default::default(),
        }),
        openai::ResponseInputContentPart::InputFile {
            detail,
            file_data,
            file_id,
            file_url,
            filename,
            ..
        } => Some(openai::ResponseToolOutputContentPart::InputFile {
            detail,
            file_data,
            file_id,
            file_url,
            filename,
            prompt_cache_breakpoint: None,
            extra: Default::default(),
        }),
        openai::ResponseInputContentPart::InputAudio { .. } => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}
