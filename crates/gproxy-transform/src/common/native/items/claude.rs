use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::NativeKind;
use crate::common::native::shape;

pub(crate) fn claude_call(
    id: String,
    input: claude::JsonObject,
    name: String,
    status: openai::ResponseItemLifecycleStatus,
) -> Result<(openai::TypedResponseItem, Option<NativeKind>), TransformError> {
    if name == "bash"
        && let Some(action) = shape::shell_action(&input)
    {
        return Ok((
            openai::TypedResponseItem::ShellCall {
                action,
                call_id: id,
                id: None,
                caller: None,
                environment: None,
                status: Some(status),
                created_by: None,
                rest: Default::default(),
            },
            Some(NativeKind::Shell),
        ));
    }
    if matches!(
        name.as_str(),
        "str_replace_editor" | "str_replace_based_edit_tool"
    ) && let Some(operation) = shape::patch_operation(&input)
    {
        return Ok((
            openai::TypedResponseItem::ApplyPatchCall {
                call_id: id,
                operation,
                status: match status {
                    openai::ResponseItemLifecycleStatus::InProgress => {
                        openai::ResponseApplyPatchCallStatus::InProgress
                    }
                    _ => openai::ResponseApplyPatchCallStatus::Completed,
                },
                id: None,
                caller: None,
                created_by: None,
                rest: Default::default(),
            },
            Some(NativeKind::ApplyPatch),
        ));
    }
    Ok((
        openai::TypedResponseItem::FunctionCall {
            arguments: serde_json::to_string(&input)?,
            call_id: id,
            name,
            id: None,
            caller: None,
            namespace: None,
            status: Some(status),
            rest: Default::default(),
        },
        None,
    ))
}

pub(crate) fn claude_result(
    block: claude::ToolResultBlock,
    kind: NativeKind,
) -> Result<openai::TypedResponseItem, TransformError> {
    let text = claude_result_text(block.content)?;
    let failed = block.is_error.unwrap_or(false);
    Ok(match kind {
        NativeKind::Shell => openai::TypedResponseItem::ShellCallOutput {
            call_id: block.tool_use_id,
            output: vec![crate::wire!(openai::ShellCallOutputContent {
                outcome: openai::ShellCallOutcome::Exit {
                    exit_code: if failed { 1 } else { 0 },
                    rest: Default::default(),
                },
                stderr: if failed { text.clone() } else { String::new() },
                stdout: if failed { String::new() } else { text },
                created_by: None,
                rest: Default::default(),
            })],
            id: None,
            caller: None,
            max_output_length: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
        NativeKind::ApplyPatch => openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id: block.tool_use_id,
            status: if failed {
                openai::ResponseApplyPatchCallOutputStatus::Failed
            } else {
                openai::ResponseApplyPatchCallOutputStatus::Completed
            },
            id: None,
            caller: None,
            output: (!text.is_empty()).then_some(text),
            created_by: None,
            rest: Default::default(),
        },
    })
}

fn claude_result_text(
    content: Option<claude::ToolResultContent>,
) -> Result<String, TransformError> {
    Ok(match content {
        None => String::new(),
        Some(claude::ToolResultContent::Text(text)) => text,
        Some(claude::ToolResultContent::Blocks(blocks)) => serde_json::to_string(&blocks)?,
        Some(claude::ToolResultContent::Raw(raw)) => raw.to_string(),
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool result",
                "future result content",
            ));
        }
    })
}
