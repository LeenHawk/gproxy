//! Encode OpenAI Responses tool items into Kiro's `conversationState` shape.
//!
//! In the Responses `input` array a prior assistant tool call is a
//! `{type:"function_call", call_id, name, arguments}` item and a tool result is
//! a `{type:"function_call_output", call_id, output}` item. Kiro instead carries
//! tool calls on the assistant turn (`assistantResponseMessage.toolUses`) and
//! tool results on the *following* user turn
//! (`userInputMessage.userInputMessageContext.toolResults`). These helpers fold
//! the flat Responses items into that shape so multi-turn tool conversations
//! replay correctly (previously a `function_call_output` errored as an unknown
//! role and tool calls were dropped). Field names mined from the v2 sample
//! `samples/kiro.rs` request model.

use serde_json::{Value, json};

use crate::channel::ChannelError;

use super::request::{assistant_message, fallback_content, user_message};

/// `{type:"function_call",...}` → a Kiro `toolUses` entry
/// `{toolUseId, name, input}`. `arguments` is a JSON string in Responses; it is
/// parsed to a value (falling back to `{}` on a parse miss).
fn tool_use_entry(item: &Value) -> Value {
    let call_id = call_id(item);
    let name = item.get("name").and_then(Value::as_str).unwrap_or_default();
    let input = match item.get("arguments") {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Some(v) => v.clone(),
        None => json!({}),
    };
    json!({ "toolUseId": call_id, "name": name, "input": input })
}

/// `{type:"function_call_output",...}` → a Kiro `toolResults` entry. Responses
/// carries no explicit success/error flag, so results are recorded as success
/// (an error surfaces in the output text).
pub(super) fn tool_result_entry(item: &Value) -> Value {
    let text = match item.get("output") {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    json!({
        "toolUseId": call_id(item),
        "content": [{ "text": text }],
        "status": "success",
    })
}

/// Append a `function_call` item to the last assistant message's `toolUses`
/// (creating an assistant message if the call has no preceding assistant turn,
/// e.g. a tool-only response). Supports parallel calls (repeated appends).
pub(super) fn append_tool_use(messages: &mut Vec<Value>, item: &Value) {
    let entry = tool_use_entry(item);
    if let Some(asst) = messages
        .last_mut()
        .and_then(|m| m.get_mut("assistantResponseMessage"))
        .and_then(Value::as_object_mut)
    {
        asst.entry("toolUses")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .expect("toolUses is an array")
            .push(entry);
        return;
    }
    let mut msg = assistant_message(".".into());
    if let Some(asst) = msg
        .get_mut("assistantResponseMessage")
        .and_then(Value::as_object_mut)
    {
        asst.insert("toolUses".into(), json!([entry]));
    }
    messages.push(msg);
}

/// Attach accumulated tool results to a `userInputMessage`'s context.
pub(super) fn attach_tool_results(user_msg: &mut Value, results: Vec<Value>) {
    if let Some(ctx) = user_msg
        .get_mut("userInputMessage")
        .and_then(Value::as_object_mut)
        .map(|ui| {
            ui.entry("userInputMessageContext")
                .or_insert_with(|| json!({ "editorState": {} }))
        })
        .and_then(Value::as_object_mut)
    {
        ctx.insert("toolResults".into(), Value::Array(results));
    }
}

/// Flush pending tool results into a synthesized user turn (used when results
/// are followed by an assistant turn, or end the conversation). No-op if empty.
pub(super) fn flush_tool_results(messages: &mut Vec<Value>, pending: &mut Vec<Value>, model: &str) {
    if pending.is_empty() {
        return;
    }
    let mut msg = user_message(fallback_content("", false), model, Vec::new());
    attach_tool_results(&mut msg, std::mem::take(pending));
    messages.push(msg);
}

/// The Responses tool-call correlation id (`call_id`, or legacy `id`).
fn call_id(item: &Value) -> &str {
    item.get("call_id")
        .or_else(|| item.get("id"))
        .and_then(Value::as_str)
        .unwrap_or_default()
}

const MAX_TOOL_DESCRIPTION_LEN: usize = 10_237;

/// Convert OpenAI `tools` into Kiro `toolSpecification` entries. Names are
/// sanitized to camelCase + length-capped; descriptions truncated; schemas
/// cleaned of `additionalProperties` / empty `required`.
pub(super) fn tools_from(tools: Option<&Value>) -> Result<Vec<Value>, ChannelError> {
    let Some(tools) = tools.and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for tool in tools {
        let function = tool.get("function");
        let tool_type = tool.get("type").and_then(Value::as_str);
        if tool_type.is_some_and(|t| t != "function")
            && function.is_none()
            && tool.get("name").is_none()
        {
            continue;
        }
        let name = function
            .and_then(|f| f.get("name"))
            .or_else(|| tool.get("name"))
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::Build("kiro tool requires a name".into()))?;
        let sanitized = shorten_name(&sanitize_name(name));
        let description = function
            .and_then(|f| f.get("description"))
            .or_else(|| tool.get("description"))
            .and_then(Value::as_str)
            .map(truncate_description)
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| format!("Tool: {sanitized}"));
        let schema = function
            .and_then(|f| f.get("parameters"))
            .or_else(|| tool.get("parameters"))
            .or_else(|| tool.get("input_schema"))
            .or_else(|| tool.get("inputSchema"));
        out.push(json!({
            "toolSpecification": {
                "name": sanitized,
                "description": description,
                "inputSchema": { "json": object_schema(schema) }
            }
        }));
    }
    Ok(out)
}

/// Attach the tool specs to the current message's `userInputMessageContext`.
pub(super) fn insert_tools(current: &mut Value, tools: Vec<Value>) {
    let Some(user_input) = current
        .get_mut("userInputMessage")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let context = user_input
        .entry("userInputMessageContext")
        .or_insert_with(|| json!({ "editorState": {} }));
    if let Some(obj) = context.as_object_mut() {
        obj.insert("tools".into(), Value::Array(tools));
    }
}

/// Coerce a JSON-schema value to a cleaned object schema (Kiro requires an
/// object root and rejects `additionalProperties`).
fn object_schema(schema: Option<&Value>) -> Value {
    let mut schema = schema
        .cloned()
        .unwrap_or_else(|| json!({ "type": "object" }));
    if !schema.is_object() {
        return json!({ "type": "object" });
    }
    clean_schema(&mut schema);
    if let Some(obj) = schema.as_object_mut() {
        obj.entry("type")
            .or_insert_with(|| Value::String("object".into()));
    }
    schema
}

/// Recursively strip `additionalProperties` and empty `required` arrays.
fn clean_schema(value: &mut Value) {
    match value {
        Value::Object(obj) => {
            obj.remove("additionalProperties");
            if obj
                .get("required")
                .is_some_and(|r| r.as_array().is_none_or(|a| a.is_empty()))
            {
                obj.remove("required");
            }
            for child in obj.values_mut() {
                clean_schema(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(clean_schema),
        _ => {}
    }
}

/// Sanitize a tool name to camelCase alphanumerics (Kiro's accepted charset).
fn sanitize_name(name: &str) -> String {
    let mut out = String::new();
    for (index, part) in name
        .split(['_', '-', ' ', '.', '/', ':'])
        .filter(|p| !p.is_empty())
        .enumerate()
    {
        let mut chars = part.chars().filter(|c| c.is_ascii_alphanumeric());
        let Some(first) = chars.next() else { continue };
        if index == 0 {
            out.push(first.to_ascii_lowercase());
        } else {
            out.push(first.to_ascii_uppercase());
        }
        out.extend(chars);
    }
    if out.is_empty() {
        "tool".to_string()
    } else {
        out
    }
}

/// Cap a tool name at 64 chars.
fn shorten_name(name: &str) -> String {
    name.chars().take(64).collect()
}

/// Cap a tool description, appending an ellipsis when truncated.
fn truncate_description(desc: &str) -> String {
    if desc.chars().count() <= MAX_TOOL_DESCRIPTION_LEN {
        return desc.to_string();
    }
    let mut out: String = desc.chars().take(MAX_TOOL_DESCRIPTION_LEN).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_parallel_tool_uses_to_one_assistant() {
        let mut messages = vec![assistant_message("calling".into())];
        append_tool_use(
            &mut messages,
            &json!({"type":"function_call","call_id":"c1","name":"get_weather","arguments":"{\"city\":\"NYC\"}"}),
        );
        append_tool_use(
            &mut messages,
            &json!({"type":"function_call","call_id":"c2","name":"get_time","arguments":"{}"}),
        );
        assert_eq!(messages.len(), 1, "both calls fold into one assistant turn");
        let uses = messages[0]["assistantResponseMessage"]["toolUses"]
            .as_array()
            .unwrap();
        assert_eq!(uses.len(), 2);
        assert_eq!(uses[0]["toolUseId"], "c1");
        assert_eq!(uses[0]["input"]["city"], "NYC");
        assert_eq!(uses[1]["name"], "get_time");
    }

    #[test]
    fn flush_synthesizes_user_turn_with_results() {
        let mut messages = Vec::new();
        let mut pending = vec![tool_result_entry(
            &json!({"type":"function_call_output","call_id":"c1","output":"sunny"}),
        )];
        flush_tool_results(&mut messages, &mut pending, "m");
        assert!(pending.is_empty());
        let results = messages[0]["userInputMessage"]["userInputMessageContext"]["toolResults"]
            .as_array()
            .unwrap();
        assert_eq!(results[0]["toolUseId"], "c1");
        assert_eq!(results[0]["content"][0]["text"], "sunny");
        assert_eq!(results[0]["status"], "success");
    }
}
