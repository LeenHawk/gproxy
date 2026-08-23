use std::collections::BTreeMap;

use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::common::{
    ResponseApplyPatchCallStatus, ResponseItemLifecycleStatus, ResponseStreamEventTypeKnown,
};
use gproxy_protocol::openai::generate_content::responses::{
    ApplyPatchOperation, ResponseItem, ResponseStreamEvent, ShellAction, ShellEnvironment,
    TypedResponseItem,
};
use serde_json::{Value, json};

#[derive(Default)]
pub(super) struct ToolAliases {
    aliases: BTreeMap<u32, Alias>,
}

struct Alias {
    kind: AliasKind,
    id: Option<String>,
    call_id: String,
    input: String,
    item: Option<ResponseItem>,
}

#[derive(Clone, Copy)]
enum AliasKind {
    Shell,
    ApplyPatch,
}

impl ToolAliases {
    pub(super) fn normalize(
        &mut self,
        mut event: ResponseStreamEvent,
    ) -> Result<Vec<ResponseStreamEvent>, ChannelError> {
        let ResponseStreamEvent::Known(known) = &mut event else {
            return Ok(vec![event]);
        };
        match known.type_ {
            ResponseStreamEventTypeKnown::ResponseOutputItemAdded => {
                let Some((index, alias)) = known
                    .output_index
                    .zip(known.item.as_deref())
                    .and_then(|(index, item)| alias(item).map(|alias| (index, alias)))
                else {
                    return Ok(vec![event]);
                };
                self.aliases.insert(index, alias);
                Ok(Vec::new())
            }
            ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDelta
            | ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDelta
                if known
                    .output_index
                    .is_some_and(|index| self.aliases.contains_key(&index)) =>
            {
                if let Some(alias) = known
                    .output_index
                    .and_then(|index| self.aliases.get_mut(&index))
                    && let Some(delta) = known.delta.as_deref()
                {
                    alias.input.push_str(delta);
                }
                Ok(Vec::new())
            }
            ResponseStreamEventTypeKnown::ResponseFunctionCallArgumentsDone
            | ResponseStreamEventTypeKnown::ResponseCustomToolCallInputDone => {
                let Some(index) = known.output_index else {
                    return Ok(vec![event]);
                };
                let Some(alias) = self.aliases.get_mut(&index) else {
                    return Ok(vec![event]);
                };
                let supplied = match alias.kind {
                    AliasKind::Shell => known.arguments.as_deref(),
                    AliasKind::ApplyPatch => known.input.as_deref(),
                };
                if let Some(supplied) = supplied {
                    supplied.clone_into(&mut alias.input);
                }
                let input = alias.input.as_str();
                let item = canonical(alias.kind, alias.id.clone(), &alias.call_id, input)?;
                alias.item = Some(item.clone());
                known.type_ = ResponseStreamEventTypeKnown::ResponseOutputItemAdded;
                known.item = Some(Box::new(item));
                known.delta = None;
                known.arguments = None;
                known.input = None;
                known.name = None;
                Ok(vec![event])
            }
            ResponseStreamEventTypeKnown::ResponseOutputItemDone => {
                if let Some(index) = known.output_index
                    && let Some(alias) = self.aliases.get(&index)
                {
                    let item = match alias.item.clone() {
                        Some(item) => Some(item),
                        None => known
                            .item
                            .as_deref()
                            .map(canonical_existing)
                            .transpose()?
                            .flatten(),
                    };
                    if let Some(item) = item {
                        known.item = Some(Box::new(item));
                    }
                } else if let Some(item) = known.item.as_deref()
                    && let Some(item) = canonical_existing(item)?
                {
                    known.item = Some(Box::new(item));
                }
                Ok(vec![event])
            }
            ResponseStreamEventTypeKnown::ResponseCompleted
            | ResponseStreamEventTypeKnown::ResponseIncomplete
            | ResponseStreamEventTypeKnown::ResponseFailed => {
                if let Some(response) = known.response.as_mut() {
                    for (index, item) in response.output.iter_mut().enumerate() {
                        let remembered = u32::try_from(index)
                            .ok()
                            .and_then(|index| self.aliases.get(&index))
                            .and_then(|alias| alias.item.clone());
                        let canonical = match remembered {
                            Some(item) => Some(item),
                            None => canonical_existing(item)?,
                        };
                        if let Some(canonical) = canonical {
                            *item = canonical;
                        }
                    }
                }
                Ok(vec![event])
            }
            _ => Ok(vec![event]),
        }
    }
}

fn alias(item: &ResponseItem) -> Option<Alias> {
    let ResponseItem::Typed(item) = item else {
        return None;
    };
    match item.as_ref() {
        TypedResponseItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
            ..
        } if name == "shell_command" => Some(Alias {
            kind: AliasKind::Shell,
            id: id.clone(),
            call_id: call_id.clone(),
            input: arguments.clone(),
            item: None,
        }),
        TypedResponseItem::CustomToolCall {
            id,
            call_id,
            name,
            input,
            ..
        } if name == "apply_patch" => Some(Alias {
            kind: AliasKind::ApplyPatch,
            id: id.clone(),
            call_id: call_id.clone(),
            input: input.clone(),
            item: None,
        }),
        _ => None,
    }
}

fn canonical_existing(item: &ResponseItem) -> Result<Option<ResponseItem>, ChannelError> {
    let Some(alias) = alias(item) else {
        return Ok(None);
    };
    let ResponseItem::Typed(item) = item else {
        return Ok(None);
    };
    let input = match item.as_ref() {
        TypedResponseItem::FunctionCall { arguments, .. } => arguments.as_str(),
        TypedResponseItem::CustomToolCall { input, .. } => input.as_str(),
        _ => return Ok(None),
    };
    canonical(alias.kind, alias.id, &alias.call_id, input).map(Some)
}

fn canonical(
    kind: AliasKind,
    id: Option<String>,
    call_id: &str,
    input: &str,
) -> Result<ResponseItem, ChannelError> {
    let item = match kind {
        AliasKind::Shell => TypedResponseItem::ShellCall {
            action: shell_action(input),
            call_id: call_id.into(),
            id,
            caller: None,
            environment: Some(ShellEnvironment {
                type_: "local".into(),
                skills: None,
                container_id: None,
                rest: Default::default(),
            }),
            status: Some(ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
        AliasKind::ApplyPatch => TypedResponseItem::ApplyPatchCall {
            call_id: call_id.into(),
            operation: patch_operation(input)?,
            status: ResponseApplyPatchCallStatus::Completed,
            id,
            caller: None,
            created_by: None,
            rest: Default::default(),
        },
    };
    Ok(ResponseItem::Typed(Box::new(item)))
}

pub(in crate::codex) fn shell_action(input: &str) -> ShellAction {
    let value: Value = serde_json::from_str(input).unwrap_or_else(|_| json!({"command":input}));
    let mut rest = value.as_object().cloned().unwrap_or_default();
    for field in ["command", "commands", "max_output_length", "timeout_ms"] {
        rest.remove(field);
    }
    let commands = value
        .get("commands")
        .and_then(Value::as_array)
        .map(|commands| {
            commands
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .or_else(|| {
            value
                .get("command")
                .and_then(Value::as_str)
                .map(|command| vec![command.into()])
        })
        .unwrap_or_default();
    ShellAction {
        commands,
        max_output_length: value
            .get("max_output_length")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        timeout_ms: value
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok()),
        rest,
    }
}

fn patch_operation(input: &str) -> Result<ApplyPatchOperation, ChannelError> {
    let mut lines = input.lines();
    lines
        .find(|line| *line == "*** Begin Patch")
        .ok_or_else(|| ChannelError::Decode("apply_patch input missing begin marker".into()))?;
    let header = lines
        .next()
        .ok_or_else(|| ChannelError::Decode("apply_patch input missing operation".into()))?;
    let (type_, path) = if let Some(path) = header.strip_prefix("*** Add File: ") {
        ("create_file", path)
    } else if let Some(path) = header.strip_prefix("*** Delete File: ") {
        ("delete_file", path)
    } else if let Some(path) = header.strip_prefix("*** Update File: ") {
        ("update_file", path)
    } else {
        return Err(ChannelError::Decode(
            "apply_patch input has an unknown operation".into(),
        ));
    };
    if path.trim().is_empty() {
        return Err(ChannelError::Decode(
            "apply_patch input has an empty path".into(),
        ));
    }
    let mut ended = false;
    let mut diff = Vec::new();
    for line in lines {
        if line == "*** End Patch" {
            ended = true;
            break;
        }
        diff.push(if type_ == "create_file" {
            line.strip_prefix('+').unwrap_or(line)
        } else {
            line
        });
    }
    if !ended {
        return Err(ChannelError::Decode(
            "apply_patch input missing end marker".into(),
        ));
    }
    let diff = diff.join("\n");
    Ok(ApplyPatchOperation {
        type_: type_.into(),
        diff: (!diff.is_empty()).then(|| format!("{diff}\n")),
        path: path.into(),
        rest: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_action_roundtrip_preserves_workdir_and_rest() {
        let action = ShellAction {
            commands: vec!["pwd".into()],
            max_output_length: Some(4096),
            timeout_ms: Some(1000),
            rest: serde_json::Map::from_iter([
                ("workdir".into(), Value::String("/repo".into())),
                ("future_shell".into(), Value::Bool(true)),
            ]),
        };
        let arguments = crate::codex::shape::tools::shell_arguments(&action);
        let restored = shell_action(&arguments.to_string());
        assert_eq!(restored, action);
    }

    #[test]
    fn malformed_patch_never_becomes_an_empty_update() {
        for input in [
            "not a patch",
            "*** Begin Patch\n*** Update File: \n*** End Patch",
            "*** Begin Patch\n*** Future File: x\n*** End Patch",
            "*** Begin Patch\n*** Update File: x",
        ] {
            assert!(patch_operation(input).is_err(), "{input}");
        }
    }
}
