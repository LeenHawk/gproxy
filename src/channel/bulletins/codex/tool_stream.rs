//! Codex-private tool aliases back to canonical Responses stream items.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::transform::TransformError;
use crate::transform::common::{SseDecoder, SseFrame};

#[derive(Default)]
pub(super) struct ToolStreamNormalizer {
    decoder: SseDecoder,
    aliases: BTreeMap<u32, Alias>,
}

#[derive(Clone, Debug)]
struct Alias {
    kind: AliasKind,
    id: String,
    call_id: String,
    item: Option<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AliasKind {
    Shell,
    ApplyPatch,
}

impl ToolStreamNormalizer {
    pub(super) fn push(&mut self, chunk: &[u8]) -> Result<Vec<u8>, TransformError> {
        let mut output = Vec::new();
        for frame in self.decoder.push(chunk)? {
            self.normalize_frame(frame, &mut output)?;
        }
        Ok(output)
    }

    pub(super) fn finish(&mut self) -> Result<Vec<u8>, TransformError> {
        let mut output = Vec::new();
        if let Some(frame) = self.decoder.finish()? {
            self.normalize_frame(frame, &mut output)?;
        }
        Ok(output)
    }

    fn normalize_frame(
        &mut self,
        frame: SseFrame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        if frame.data.trim() == "[DONE]" {
            output.extend_from_slice(frame.encode().as_bytes());
            return Ok(());
        }
        let Ok(mut event) = serde_json::from_str::<Value>(&frame.data) else {
            output.extend_from_slice(frame.encode().as_bytes());
            return Ok(());
        };
        let kind = event.get("type").and_then(Value::as_str);
        match kind {
            Some("response.output_item.added") => {
                let Some(index) = output_index(&event) else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let Some(item) = event.get("item") else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let alias_kind = match (
                    item.get("type").and_then(Value::as_str),
                    item.get("name").and_then(Value::as_str),
                ) {
                    (Some("function_call"), Some("shell_command")) => AliasKind::Shell,
                    (Some("custom_tool_call"), Some("apply_patch")) => AliasKind::ApplyPatch,
                    _ => return encode_value(event, frame.event.as_deref(), output),
                };
                self.aliases.insert(
                    index,
                    Alias {
                        kind: alias_kind,
                        id: item
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        call_id: item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        item: None,
                    },
                );
                Ok(())
            }
            Some("response.function_call_arguments.delta")
            | Some("response.custom_tool_call_input.delta")
                if output_index(&event).is_some_and(|index| self.aliases.contains_key(&index)) =>
            {
                Ok(())
            }
            Some("response.function_call_arguments.done")
            | Some("response.custom_tool_call_input.done") => {
                let Some(index) = output_index(&event) else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let Some(alias) = self.aliases.get_mut(&index) else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let input = match alias.kind {
                    AliasKind::Shell => event
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    AliasKind::ApplyPatch => event
                        .get("input")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                };
                let item = canonical_item(alias.kind, &alias.id, &alias.call_id, input);
                alias.item = Some(item.clone());
                let mut added = json!({
                    "type": "response.output_item.added",
                    "output_index": index,
                    "item": item
                });
                copy_sequence_number(&event, &mut added);
                encode_value(added, Some("response.output_item.added"), output)
            }
            Some("response.output_item.done") => {
                let Some(index) = output_index(&event) else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let Some(alias) = self.aliases.get(&index) else {
                    return encode_value(event, frame.event.as_deref(), output);
                };
                let item = alias.item.clone().unwrap_or_else(|| {
                    let input = event
                        .get("item")
                        .and_then(|item| {
                            item.get(match alias.kind {
                                AliasKind::Shell => "arguments",
                                AliasKind::ApplyPatch => "input",
                            })
                        })
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    canonical_item(alias.kind, &alias.id, &alias.call_id, input)
                });
                event["item"] = item;
                encode_value(event, frame.event.as_deref(), output)
            }
            Some("response.completed" | "response.incomplete" | "response.failed") => {
                if let Some(items) = event
                    .get_mut("response")
                    .and_then(Value::as_object_mut)
                    .and_then(|response| response.get_mut("output"))
                    .and_then(Value::as_array_mut)
                {
                    for item in items {
                        if let Some(rewritten) = canonical_completed_item(item) {
                            *item = rewritten;
                        }
                    }
                }
                encode_value(event, frame.event.as_deref(), output)
            }
            _ => encode_value(event, frame.event.as_deref(), output),
        }
    }
}

fn output_index(event: &Value) -> Option<u32> {
    event
        .get("output_index")?
        .as_u64()
        .map(|value| u32::try_from(value).unwrap_or(u32::MAX))
}

fn canonical_completed_item(item: &Value) -> Option<Value> {
    let kind = match (
        item.get("type").and_then(Value::as_str),
        item.get("name").and_then(Value::as_str),
    ) {
        (Some("function_call"), Some("shell_command")) => AliasKind::Shell,
        (Some("custom_tool_call"), Some("apply_patch")) => AliasKind::ApplyPatch,
        _ => return None,
    };
    Some(canonical_item(
        kind,
        item.get("id").and_then(Value::as_str).unwrap_or_default(),
        item.get("call_id")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        item.get(match kind {
            AliasKind::Shell => "arguments",
            AliasKind::ApplyPatch => "input",
        })
        .and_then(Value::as_str)
        .unwrap_or_default(),
    ))
}

fn canonical_item(kind: AliasKind, id: &str, call_id: &str, input: &str) -> Value {
    let mut item = match kind {
        AliasKind::Shell => json!({
            "type": "shell_call",
            "call_id": call_id,
            "action": shell_action(input),
            "environment": {"type": "local"},
            "status": "completed"
        }),
        AliasKind::ApplyPatch => json!({
            "type": "apply_patch_call",
            "call_id": call_id,
            "operation": patch_operation(input),
            "status": "completed"
        }),
    };
    if !id.is_empty() {
        item["id"] = Value::String(id.to_owned());
    }
    item
}

fn shell_action(arguments: &str) -> Value {
    let arguments =
        serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({"command": arguments}));
    let commands = arguments
        .get("commands")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| {
            arguments
                .get("command")
                .and_then(Value::as_str)
                .map(|command| vec![Value::String(command.to_owned())])
        })
        .unwrap_or_default();
    let mut action = json!({"commands": commands});
    if let Some(timeout) = arguments.get("timeout_ms").cloned() {
        action["timeout_ms"] = timeout;
    }
    action
}

fn patch_operation(input: &str) -> Value {
    let mut lines = input.lines();
    let _ = lines.find(|line| *line == "*** Begin Patch");
    let header = lines.next().unwrap_or_default();
    if let Some(path) = header.strip_prefix("*** Add File: ") {
        let mut text = String::new();
        for line in lines.take_while(|line| *line != "*** End Patch") {
            let line = line.strip_prefix('+').unwrap_or(line);
            text.push_str(line);
            text.push('\n');
        }
        return json!({"type":"create_file", "path":path, "diff":text});
    }
    if let Some(path) = header.strip_prefix("*** Delete File: ") {
        return json!({"type":"delete_file", "path":path});
    }
    let path = header.strip_prefix("*** Update File: ").unwrap_or_default();
    let diff = lines
        .take_while(|line| *line != "*** End Patch")
        .collect::<Vec<_>>()
        .join("\n");
    json!({"type":"update_file", "path":path, "diff":format!("{diff}\n")})
}

fn copy_sequence_number(source: &Value, target: &mut Value) {
    if let Some(sequence) = source.get("sequence_number").cloned() {
        target["sequence_number"] = sequence;
    }
}

fn encode_value(
    value: Value,
    fallback_event: Option<&str>,
    output: &mut Vec<u8>,
) -> Result<(), TransformError> {
    let event = value
        .get("type")
        .and_then(Value::as_str)
        .or(fallback_event)
        .unwrap_or("message");
    let data = serde_json::to_string(&value).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })?;
    output.extend_from_slice(SseFrame::event(event, data).encode().as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_codex_shell_and_apply_patch_aliases() {
        let input = concat!(
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc1\",\"call_id\":\"c1\",\"name\":\"shell_command\",\"arguments\":\"\"}}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":0,\"item_id\":\"fc1\",\"name\":\"shell_command\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\"}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"custom_tool_call\",\"id\":\"ct1\",\"call_id\":\"c2\",\"name\":\"apply_patch\",\"input\":\"\"}}\n\n",
            "event: response.custom_tool_call_input.done\n",
            "data: {\"type\":\"response.custom_tool_call_input.done\",\"output_index\":1,\"item_id\":\"ct1\",\"input\":\"*** Begin Patch\\n*** Add File: /tmp/a\\n+ok\\n*** End Patch\\n\"}\n\n"
        );
        let mut normalizer = ToolStreamNormalizer::default();
        let output = String::from_utf8(normalizer.push(input.as_bytes()).unwrap()).unwrap();
        assert!(output.contains("\"type\":\"shell_call\""));
        assert!(output.contains("\"commands\":[\"pwd\"]"));
        assert!(output.contains("\"environment\":{\"type\":\"local\"}"));
        assert!(output.contains("\"type\":\"apply_patch_call\""));
        assert!(output.contains("\"type\":\"create_file\""));
        for data in output.lines().filter_map(|line| line.strip_prefix("data: ")) {
            serde_json::from_str::<crate::protocol::openai::ResponseStreamEvent>(data)
                .expect("normalized event must remain valid Responses wire");
        }
    }
}
