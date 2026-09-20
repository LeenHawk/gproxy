use super::invalid;
use crate::channel::ChannelError;
use gproxy_protocol::wire::openai::responses::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Only tools actually adapted by this request may be decoded as native tools.
#[derive(Debug, Default, Clone)]
pub(in crate::channels::codex) struct Aliases {
    pub tools: BTreeMap<String, NativeTool>,
    pub ids: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::channels::codex) enum NativeTool {
    Shell,
    LocalShell,
    ApplyPatch,
}

const APPLY_PATCH_GRAMMAR: &str = r#"start: begin_patch hunk+ end_patch
begin_patch: "*** Begin Patch" LF
end_patch: "*** End Patch" LF?
hunk: add_hunk | delete_hunk | update_hunk
add_hunk: "*** Add File: " filename LF add_line+
delete_hunk: "*** Delete File: " filename LF
update_hunk: "*** Update File: " filename LF change_move? change?
filename: /(.+)/
add_line: "+" /(.*)/ LF -> line
change_move: "*** Move to: " filename LF
change: (change_context | change_line)+ eof_line?
change_context: ("@@" | "@@ " /(.+)/) LF
change_line: ("+" | "-" | " ") /(.*)/ LF
eof_line: "*** End of File" LF
%import common.LF
"#;

pub(super) fn normalize_definitions(
    tools: &mut Option<Vec<Tool>>,
    choice: &mut Option<ToolChoice>,
) -> Result<Aliases, ChannelError> {
    let mut aliases = Aliases::default();
    let Some(tools) = tools else {
        return Ok(aliases);
    };
    let mut names: BTreeSet<String> = tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::Function(tool) => Some(tool.name.clone()),
            Tool::Custom(tool) => Some(tool.name.clone()),
            Tool::Namespace(tool) => Some(tool.name.clone()),
            _ => None,
        })
        .collect();
    for tool in tools {
        let kind = match tool {
            Tool::Shell(_) => Some(NativeTool::Shell),
            Tool::LocalShell(_) => Some(NativeTool::LocalShell),
            Tool::ApplyPatch(_) => Some(NativeTool::ApplyPatch),
            Tool::ToolSearch(tool) if tool.execution == Some(ToolExecution::Client) => {
                tool.description.get_or_insert_with(|| {
                    Some("Search deferred tools using a regular expression".into())
                });
                tool.parameters.get_or_insert_with(|| Some(json!({
                    "type":"object", "properties": {
                        "query":{"type":"string","description":"Search query for deferred tools."},
                        "limit":{"type":"number","description":"Maximum tools to return."}
                    }, "required":["query"], "additionalProperties":false
                })));
                None
            }
            _ => None,
        };
        let Some(kind) = kind else {
            continue;
        };
        let base = if kind == NativeTool::ApplyPatch {
            "apply_patch"
        } else {
            "shell_command"
        };
        let mut name = base.to_owned();
        let mut suffix = 1;
        while !names.insert(name.clone()) {
            name = format!("{base}_{suffix}");
            suffix += 1;
        }
        *tool = serde_json::from_value(if kind == NativeTool::ApplyPatch {
            json!({"type":"custom","name":name,
                "description":"The apply_patch tool can be used to edit files. This is a FREEFORM tool, so do not wrap the patch in JSON.",
                "format":{"type":"grammar","syntax":"lark","definition":APPLY_PATCH_GRAMMAR}})
        } else {
            json!({"type":"function","name":name,"strict":false,
                "description":"Runs a shell command and returns its output.",
                "parameters":{"type":"object","properties":{
                    "command":{"type":"string","description":"Shell script to run."},
                    "workdir":{"type":"string","description":"Working directory for the command."},
                    "timeout_ms":{"type":"number","description":"Maximum command runtime."}
                },"required":["command"],"additionalProperties":false}})
        }).map_err(invalid)?;
        aliases.tools.insert(name, kind);
    }
    if let Some(value) = choice {
        let kind = match value {
            ToolChoice::Shell(_) => Some(NativeTool::Shell),
            ToolChoice::ApplyPatch(_) => Some(NativeTool::ApplyPatch),
            _ => None,
        };
        if let Some(kind) = kind
            && let Some((name, _)) = aliases
                .tools
                .iter()
                .find(|(_, candidate)| **candidate == kind)
        {
            *value = serde_json::from_value(json!({
                "type": if kind == NativeTool::ApplyPatch { "custom" } else { "function" }, "name":name
            })).map_err(invalid)?;
        }
    }
    Ok(aliases)
}

pub(super) fn normalize_history(
    items: &mut [InputItem],
    aliases: &mut Aliases,
) -> Result<(), ChannelError> {
    for item in items {
        let (kind, mut value) = match item {
            InputItem::ShellCall(call) => (
                NativeTool::Shell,
                json!({
                    "type":"function_call", "call_id":call.call_id, "arguments":shell_arguments(&call.action).to_string(),
                    "status":call.status, "id":call.id
                }),
            ),
            InputItem::LocalShellCall(call) => (
                NativeTool::LocalShell,
                json!({
                    "type":"function_call", "call_id":call.call_id, "id":call.id, "status":call.status,
                    "arguments":json!({"command":call.action.command.join("\n"),"timeout_ms":call.action.timeout_ms,
                        "workdir":call.action.working_directory}).to_string()
                }),
            ),
            InputItem::ShellCallOutput(call) => (
                NativeTool::Shell,
                json!({
                    "type":"function_call_output", "call_id":call.call_id, "id":call.id, "status":call.status,
                    "output":call.output.iter().flat_map(|part| [&part.stdout,&part.stderr]).filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join("\n")
                }),
            ),
            InputItem::LocalShellCallOutput(call) => (
                NativeTool::LocalShell,
                json!({
                    "type":"function_call_output", "call_id":call.rest.get("call_id").and_then(Value::as_str).unwrap_or(&call.id),
                    "id":call.id, "status":call.status, "output":call.output
                }),
            ),
            InputItem::ApplyPatchCall(call) => (
                NativeTool::ApplyPatch,
                json!({
                    "type":"custom_tool_call", "call_id":call.call_id, "id":call.id, "input":patch_text(&call.operation)
                }),
            ),
            InputItem::ApplyPatchCallOutput(call) => (
                NativeTool::ApplyPatch,
                json!({
                    "type":"custom_tool_call_output", "call_id":call.call_id, "id":call.id, "status":call.status,
                    "output":call.output.as_ref().and_then(Option::as_ref).ok_or_else(|| invalid("apply_patch_call_output.output missing"))?
                }),
            ),
            _ => continue,
        };
        let is_call = matches!(
            value["type"].as_str(),
            Some("function_call" | "custom_tool_call")
        );
        let object = value.as_object_mut().expect("constructed object");
        object.retain(|_, value| !value.is_null());
        if is_call {
            let name = aliases
                .tools
                .iter()
                .find(|(_, candidate)| **candidate == kind)
                .map(|(name, _)| name.as_str())
                .unwrap_or(if kind == NativeTool::ApplyPatch {
                    "apply_patch"
                } else {
                    "shell_command"
                });
            object.insert("name".into(), Value::String(name.into()));
            if let Some(id) = object.get("id").and_then(Value::as_str) {
                let mapped = mapped_id(
                    id,
                    if kind == NativeTool::ApplyPatch {
                        "ctc_"
                    } else {
                        "fc_"
                    },
                );
                aliases.ids.insert(mapped.clone(), id.to_owned());
                object.insert("id".into(), Value::String(mapped));
            }
        }
        *item = serde_json::from_value(value).map_err(invalid)?;
    }
    Ok(())
}

pub(in crate::channels::codex) fn shell_arguments(action: &ShellAction) -> Value {
    let mut value = Value::Object(action.rest.clone());
    value["command"] = Value::String(action.commands.join("\n"));
    if let Some(timeout) = action.timeout_ms {
        value["timeout_ms"] = json!(timeout);
    }
    if let Some(max) = action.max_output_length {
        value["max_output_length"] = json!(max);
    }
    value
}

fn patch_text(operation: &ApplyPatchOperation) -> String {
    let mut patch = String::from("*** Begin Patch\n");
    match operation {
        ApplyPatchOperation::Create(op) => {
            patch.push_str(&format!("*** Add File: {}\n", op.path));
            for line in op.diff.lines() {
                patch.push('+');
                patch.push_str(line);
                patch.push('\n');
            }
        }
        ApplyPatchOperation::Delete(op) => {
            patch.push_str(&format!("*** Delete File: {}\n", op.path))
        }
        ApplyPatchOperation::Update(op) => {
            patch.push_str(&format!("*** Update File: {}\n{}", op.path, op.diff));
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
        }
    }
    patch.push_str("*** End Patch\n");
    patch
}

fn mapped_id(value: &str, prefix: &str) -> String {
    if value.starts_with(prefix) {
        return value.into();
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{prefix}{hash:016x}")
}
