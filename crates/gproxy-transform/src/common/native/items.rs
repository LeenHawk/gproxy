use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::shape;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeKind {
    Shell,
    ApplyPatch,
}

pub(crate) struct ClaudeCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input: claude::JsonObject,
    pub(crate) item_id: Option<String>,
    pub(crate) rest: openai::Rest,
}

pub(crate) fn is_buffered_native(name: &str) -> bool {
    matches!(
        name,
        "bash" | "str_replace_editor" | "str_replace_based_edit_tool"
    )
}

pub(crate) fn claude_call(
    id: String,
    input: claude::JsonObject,
    name: String,
    mut rest: openai::Rest,
    status: openai::ResponseItemLifecycleStatus,
) -> Result<(openai::TypedResponseItem, Option<NativeKind>), TransformError> {
    let item_id = take_item_id(&mut rest)?;
    if name == "bash"
        && let Some(action) = shape::shell_action(&input)
    {
        return Ok((
            openai::TypedResponseItem::ShellCall {
                action,
                call_id: id,
                id: item_id,
                caller: None,
                environment: None,
                status: Some(status),
                created_by: None,
                rest,
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
                id: item_id,
                caller: None,
                created_by: None,
                rest,
            },
            Some(NativeKind::ApplyPatch),
        ));
    }
    Ok((
        openai::TypedResponseItem::FunctionCall {
            arguments: serde_json::to_string(&input)?,
            call_id: id,
            name,
            id: item_id,
            caller: None,
            namespace: None,
            status: Some(status),
            rest,
        },
        None,
    ))
}

pub(crate) fn claude_result(
    mut block: claude::ToolResultBlock,
    kind: NativeKind,
) -> Result<openai::TypedResponseItem, TransformError> {
    let item_id = take_item_id(&mut block.rest)?;
    let text = claude_result_text(block.content)?;
    let failed = block.is_error.unwrap_or(false);
    Ok(match kind {
        NativeKind::Shell => openai::TypedResponseItem::ShellCallOutput {
            call_id: block.tool_use_id,
            output: vec![openai::ShellCallOutputContent {
                outcome: openai::ShellCallOutcome {
                    type_: "exit".into(),
                    exit_code: Some(if failed { 1 } else { 0 }),
                    rest: Default::default(),
                },
                stderr: if failed { text.clone() } else { String::new() },
                stdout: if failed { String::new() } else { text },
                created_by: None,
                rest: Default::default(),
            }],
            id: item_id,
            caller: None,
            max_output_length: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: block.rest,
        },
        NativeKind::ApplyPatch => openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id: block.tool_use_id,
            status: if failed {
                openai::ResponseApplyPatchCallOutputStatus::Failed
            } else {
                openai::ResponseApplyPatchCallOutputStatus::Completed
            },
            id: item_id,
            caller: None,
            output: (!text.is_empty()).then_some(text),
            created_by: None,
            rest: block.rest,
        },
    })
}

pub(crate) fn openai_call(
    item: openai::TypedResponseItem,
) -> Result<Option<ClaudeCall>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::LocalShellCall {
            id,
            action,
            call_id,
            rest,
            ..
        } => {
            let fallback = action.clone();
            let (name, input) = match shape::local_bash_input(action)? {
                Some(input) => ("bash", input),
                None => (
                    "local_shell",
                    shape::value_object(serde_json::to_value(fallback)?),
                ),
            };
            Some(ClaudeCall {
                id: call_id,
                name: name.into(),
                input,
                item_id: Some(id),
                rest,
            })
        }
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            id,
            environment,
            rest,
            ..
        } => {
            let fallback_action = action.clone();
            let fallback_environment = environment.clone();
            let (name, input) = match shape::bash_input(action, environment)? {
                Some(input) => ("bash", input),
                None => {
                    let mut input = shape::value_object(serde_json::to_value(fallback_action)?);
                    if let Some(environment) = fallback_environment {
                        input.insert("environment".into(), serde_json::to_value(environment)?);
                    }
                    ("shell", input)
                }
            };
            Some(ClaudeCall {
                id: call_id,
                name: name.into(),
                input,
                item_id: id,
                rest,
            })
        }
        openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            id,
            rest,
            ..
        } => {
            let fallback = operation.clone();
            let (name, input) = match shape::editor_input(operation) {
                Some(input) => ("str_replace_based_edit_tool", input),
                None => (
                    "apply_patch",
                    shape::value_object(serde_json::to_value(fallback)?),
                ),
            };
            Some(ClaudeCall {
                id: call_id,
                name: name.into(),
                input,
                item_id: id,
                rest,
            })
        }
        openai::TypedResponseItem::ComputerCall {
            id,
            call_id,
            action,
            actions,
            mut rest,
            ..
        } => {
            let input = if let Some(action) = action {
                shape::value_object(serde_json::to_value(action)?)
            } else if let Some(actions) = actions {
                [("actions".into(), serde_json::to_value(actions)?)]
                    .into_iter()
                    .collect()
            } else {
                return Err(TransformError::shape(
                    "OpenAI computer call",
                    "both action and actions are missing",
                ));
            };
            rest.insert("openai_native_tool".into(), "computer_call".into());
            Some(ClaudeCall {
                id: call_id,
                name: "computer".into(),
                input,
                item_id: Some(id),
                rest,
            })
        }
        openai::TypedResponseItem::WebSearchCall {
            id,
            action,
            mut rest,
            ..
        } => {
            rest.insert("openai_native_tool".into(), "web_search_call".into());
            Some(ClaudeCall {
                id: id.clone(),
                name: "web_search".into(),
                input: shape::value_object(serde_json::to_value(action)?),
                item_id: Some(id),
                rest,
            })
        }
        openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            mut rest,
            ..
        } => {
            let mut input = claude::JsonObject::new();
            if let Some(code) = code {
                input.insert("code".into(), code.into());
            }
            input.insert("container_id".into(), container_id.into());
            rest.insert("openai_native_tool".into(), "code_interpreter_call".into());
            Some(ClaudeCall {
                id: id.clone(),
                name: "code_execution".into(),
                input,
                item_id: Some(id),
                rest,
            })
        }
        openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            mut rest,
            ..
        } => {
            let item_id = id;
            let id = call_id.ok_or_else(|| {
                TransformError::shape("OpenAI tool_search call", "call_id is missing")
            })?;
            let name = match execution {
                Some(openai::ToolSearchExecution::Client) => "tool_search_tool_regex",
                Some(openai::ToolSearchExecution::Server) => "tool_search_tool_bm25",
                Some(openai::ToolSearchExecution::Unknown(value)) => {
                    return Err(TransformError::unsupported(
                        "OpenAI tool_search execution",
                        value,
                    ));
                }
                None => {
                    return Err(TransformError::shape(
                        "OpenAI tool_search call",
                        "execution is missing",
                    ));
                }
            };
            rest.insert("openai_native_tool".into(), "tool_search_call".into());
            Some(ClaudeCall {
                id: id.clone(),
                name: name.into(),
                input: shape::value_object(arguments),
                item_id,
                rest,
            })
        }
        openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            mut rest,
            ..
        } => {
            rest.insert("openai_native_tool".into(), "mcp_call".into());
            Some(ClaudeCall {
                id: id.clone(),
                name,
                input: shape::arguments_object(&arguments)?,
                item_id: Some(id),
                rest,
            })
        }
        openai::TypedResponseItem::Program {
            id,
            call_id,
            code,
            fingerprint,
            mut rest,
        } => {
            let input = [
                ("code".into(), code.into()),
                ("fingerprint".into(), fingerprint.into()),
            ]
            .into_iter()
            .collect();
            rest.insert("openai_native_tool".into(), "program".into());
            Some(ClaudeCall {
                id: call_id,
                name: "program".into(),
                input,
                item_id: Some(id),
                rest,
            })
        }
        _ => None,
    })
}

pub(crate) fn request_block(mut call: ClaudeCall) -> claude::ContentBlockParam {
    preserve_item_id(&mut call.rest, call.item_id);
    claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: call.rest,
    })
}

pub(crate) fn response_block(mut call: ClaudeCall) -> claude::ResponseContentBlock {
    preserve_item_id(&mut call.rest, call.item_id);
    claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        rest: call.rest,
    })
}

pub(crate) fn item_id(item: &openai::TypedResponseItem) -> Option<String> {
    match item {
        openai::TypedResponseItem::LocalShellCall { id, .. } => Some(id.clone()),
        openai::TypedResponseItem::ShellCall { id, .. }
        | openai::TypedResponseItem::ApplyPatchCall { id, .. } => id.clone(),
        openai::TypedResponseItem::ComputerCall { id, .. }
        | openai::TypedResponseItem::WebSearchCall { id, .. }
        | openai::TypedResponseItem::CodeInterpreterCall { id, .. }
        | openai::TypedResponseItem::McpCall { id, .. }
        | openai::TypedResponseItem::Program { id, .. } => Some(id.clone()),
        openai::TypedResponseItem::ToolSearchCall { id, .. } => id.clone(),
        _ => None,
    }
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

fn take_item_id(rest: &mut openai::Rest) -> Result<Option<String>, TransformError> {
    rest.remove("openai_item_id")
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

fn preserve_item_id(rest: &mut openai::Rest, item_id: Option<String>) {
    if let Some(item_id) = item_id {
        rest.insert("openai_item_id".into(), item_id.into());
    }
}
