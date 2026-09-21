//! Tool calls, tool results and tool declarations inside the envelope.

use super::{content, invalid};
use crate::channel::ChannelError;
use serde_json::{Value, json};

/// The upstream caps a tool description; v3 measured the limit at 10237
/// characters (`request/tools.rs`).
const MAX_DESCRIPTION_CHARS: usize = 10_237;
/// And a tool name at 64.
const MAX_NAME_CHARS: usize = 64;

/// A `function_call_output` as the envelope's tool result.
pub(super) fn result(item: &Value) -> Result<Value, ChannelError> {
    let output = item
        .get("output")
        .ok_or_else(|| invalid("Kiro tool output has no output"))?;
    let output = output
        .as_str()
        .map_or_else(|| output.to_string(), str::to_owned);
    Ok(json!({
        "toolUseId": call_id(item)?,
        "content": [{"text": output}],
        "status": "success",
    }))
}

/// A `function_call` joins the assistant turn before it; without one, an
/// assistant turn is opened to hold it.
pub(super) fn append_call(messages: &mut Vec<Value>, item: &Value) -> Result<(), ChannelError> {
    let arguments = item
        .get("arguments")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Kiro tool call has no arguments"))?;
    let input: Value = serde_json::from_str(arguments)
        .map_err(|error| invalid(format!("Kiro tool arguments are not JSON: {error}")))?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| invalid("Kiro tool call has no name"))?;
    let entry = json!({"toolUseId": call_id(item)?, "name": name, "input": input});
    if let Some(message) = messages
        .last_mut()
        .and_then(|message| message.get_mut("assistantResponseMessage"))
        .and_then(Value::as_object_mut)
    {
        message
            .entry("toolUses")
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| invalid("Kiro toolUses is not an array"))?
            .push(entry);
        return Ok(());
    }
    let mut message = content::assistant(".".into());
    message["assistantResponseMessage"]["toolUses"] = Value::Array(vec![entry]);
    messages.push(message);
    Ok(())
}

pub(super) fn attach_results(message: &mut Value, results: Vec<Value>) {
    if !results.is_empty() {
        message["userInputMessage"]["userInputMessageContext"]["toolResults"] =
            Value::Array(results);
    }
}

/// Results that no user turn followed still need one to ride on.
pub(super) fn flush_results(messages: &mut Vec<Value>, results: &mut Vec<Value>, model: &str) {
    if results.is_empty() {
        return;
    }
    let mut message = content::user(".", model, Vec::new());
    attach_results(&mut message, std::mem::take(results));
    messages.push(message);
}

/// Responses tool declarations as `toolSpecification` entries. Anything that
/// is not a function tool has no counterpart and is left out.
pub(super) fn definitions(tools: Option<&Value>) -> Result<Vec<Value>, ChannelError> {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut output = Vec::new();
    for tool in tools {
        if tool
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind != "function")
        {
            continue;
        }
        let function = tool.get("function").unwrap_or(tool);
        let Some(name) = function.get("name").and_then(Value::as_str) else {
            continue;
        };
        let description = function
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .chars()
            .take(MAX_DESCRIPTION_CHARS)
            .collect::<String>();
        let mut schema = function
            .get("parameters")
            .or_else(|| function.get("input_schema"))
            .cloned()
            .unwrap_or_else(|| json!({"type": "object"}));
        if !schema.is_object() {
            schema = json!({"type": "object"});
        }
        clean_schema(&mut schema);
        if let Some(schema) = schema.as_object_mut() {
            schema
                .entry("type")
                .or_insert_with(|| Value::String("object".into()));
        }
        output.push(json!({"toolSpecification": {
            "name": sanitize(name),
            "description": description,
            "inputSchema": {"json": schema},
        }}));
    }
    Ok(output)
}

pub(super) fn attach_definitions(current: &mut Value, tools: Vec<Value>) {
    if !tools.is_empty() {
        current["userInputMessage"]["userInputMessageContext"]["tools"] = Value::Array(tools);
    }
}

/// The upstream's schema validator rejects `additionalProperties` and an empty
/// `required` (v3 `request/tools.rs::clean_schema`).
fn clean_schema(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.remove("additionalProperties");
            if object
                .get("required")
                .is_some_and(|value| value.as_array().is_none_or(Vec::is_empty))
            {
                object.remove("required");
            }
            for value in object.values_mut() {
                clean_schema(value);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(clean_schema),
        _ => {}
    }
}

/// The upstream takes lowerCamelCase alphanumeric tool names only.
fn sanitize(name: &str) -> String {
    let mut output = String::new();
    for (index, part) in name
        .split(['_', '-', ' ', '.', '/', ':'])
        .filter(|part| !part.is_empty())
        .enumerate()
    {
        let mut chars = part.chars().filter(char::is_ascii_alphanumeric);
        if let Some(first) = chars.next() {
            output.push(if index == 0 {
                first.to_ascii_lowercase()
            } else {
                first.to_ascii_uppercase()
            });
            output.extend(chars);
        }
    }
    if output.is_empty() {
        "tool".into()
    } else {
        output.chars().take(MAX_NAME_CHARS).collect()
    }
}

fn call_id(item: &Value) -> Result<&str, ChannelError> {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| invalid("Kiro tool call has no id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_lower_camel_case_and_survive_being_unnameable() {
        assert_eq!(sanitize("read_file"), "readFile");
        assert_eq!(sanitize("mcp::web.search-v2"), "mcpWebSearchV2");
        assert_eq!(sanitize("!!!"), "tool");
        assert_eq!(sanitize(&"a".repeat(200)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn a_non_function_tool_has_no_counterpart() {
        let tools = json!([
            {"type": "web_search"},
            {"type": "function", "name": "f", "parameters": "not an object"},
        ]);
        let defined = definitions(Some(&tools)).unwrap();
        assert_eq!(defined.len(), 1);
        assert_eq!(
            defined[0]["toolSpecification"]["inputSchema"]["json"],
            json!({"type": "object"})
        );
        assert_eq!(
            defined[0]["toolSpecification"]["description"], "f",
            "the name stands in for a missing description"
        );
    }
}
