use serde_json::{Value, json};

use crate::error::CoreError;

pub(super) const PROMPT_FIELDS: &[&str] = &[
    "system",
    "messages",
    "tools",
    "tool_choice",
    "thinking",
    "cache_control",
    "output_config",
    "mcp_servers",
    "context_management",
    "container",
];

pub(super) fn completed_tools(response: &Value) -> bool {
    let Some(content) = response.get("content").and_then(Value::as_array) else {
        return false;
    };
    content
        .iter()
        .filter(|block| {
            matches!(
                block["type"].as_str(),
                Some("server_tool_use" | "mcp_tool_use")
            )
        })
        .any(|call| {
            content.iter().any(|result| {
                result
                    .get("tool_use_id")
                    .is_some_and(|id| Some(id) == call.get("id"))
                    && result["type"]
                        .as_str()
                        .is_some_and(|kind| kind.ends_with("tool_result"))
            })
        })
}

pub(super) fn continuation(body: &Value, response: &Value) -> Result<Value, CoreError> {
    let mut body = body.clone();
    let mut content = response
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let completed = content
        .iter()
        .filter(|block| block["type"] == "tool_result")
        .filter_map(|block| block["tool_use_id"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    content.retain(|block| {
        block["type"] != "tool_use"
            || block["id"]
                .as_str()
                .is_some_and(|id| completed.iter().any(|done| done == id))
    });
    if let Some(last) = content.last_mut().filter(|block| block["type"] == "text")
        && let Some(text) = last["text"].as_str()
    {
        last["text"] = json!(text.trim_end());
    }
    let messages = body
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| CoreError::Transform("fallback continuation requires messages".into()))?;
    messages.push(json!({"role":"assistant","content":content}));
    Ok(body)
}

pub(super) fn message(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default()
}
