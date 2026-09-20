mod patch;
mod shell;

use super::event::invalid;
use crate::{
    channel::ChannelError,
    channels::codex::shape::tools::{Aliases, NativeTool},
};
use gproxy_protocol::wire::openai::responses::{ApplyPatchCall, LocalShellCall, ShellCall};
use serde_json::{Value, json};

pub(super) fn kind(aliases: &Aliases, item: &Value) -> Option<NativeTool> {
    let kind = *aliases.tools.get(item.get("name")?.as_str()?)?;
    match (kind, item.get("type")?.as_str()?) {
        (NativeTool::ApplyPatch, "custom_tool_call") => Some(kind),
        (NativeTool::Shell | NativeTool::LocalShell, "function_call") => Some(kind),
        _ => None,
    }
}

pub(super) fn restore(aliases: &Aliases, mut item: Value) -> Result<Value, ChannelError> {
    if let Some(id) = item.get("id").and_then(Value::as_str)
        && let Some(original) = aliases.ids.get(id)
    {
        item["id"] = json!(original);
    }
    if matches!(
        item.get("status").and_then(Value::as_str),
        Some("incomplete" | "failed" | "in_progress")
    ) {
        return Ok(item);
    }
    let Some(kind) = kind(aliases, &item) else {
        return Ok(item);
    };
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("tool call_id missing"))?
        .to_owned();
    let id = item.get("id").and_then(Value::as_str).map(str::to_owned);
    let input_key = if kind == NativeTool::ApplyPatch {
        "input"
    } else {
        "arguments"
    };
    let input = item
        .get(input_key)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("tool input missing"))?;
    let payload = match kind {
        NativeTool::Shell => {
            let action = shell::shell_action(input)?;
            json!({"type":"shell_call","call_id":call_id,"action":action,"environment":{"type":"local"},"status":"completed"})
        }
        NativeTool::LocalShell => {
            let action = shell::shell_action(input)?;
            let mut local = json!({"type":"exec","command":action.commands,"env":{},"timeout_ms":action.timeout_ms});
            if let Some(workdir) = action.rest.get("workdir") {
                local["working_directory"] = workdir.clone();
            }
            json!({"type":"local_shell_call","id":id.as_ref().unwrap_or(&call_id),"call_id":call_id,"action":local,"status":"completed"})
        }
        NativeTool::ApplyPatch => json!({"type":"apply_patch_call","call_id":call_id,
            "operation":patch::patch_operation(input)?,"status":"completed"}),
    };
    let object = item
        .as_object_mut()
        .ok_or_else(|| invalid("tool item must be an object"))?;
    for field in ["name", "arguments", "input", "namespace"] {
        object.remove(field);
    }
    object.extend(payload.as_object().unwrap().clone());
    // Validate the complete restored item against v4's native wire contracts.
    match kind {
        NativeTool::Shell => {
            serde_json::from_value::<ShellCall>(item.clone()).map_err(invalid)?;
        }
        NativeTool::LocalShell => {
            serde_json::from_value::<LocalShellCall>(item.clone()).map_err(invalid)?;
        }
        NativeTool::ApplyPatch => {
            serde_json::from_value::<ApplyPatchCall>(item.clone()).map_err(invalid)?;
        }
    }
    Ok(item)
}
