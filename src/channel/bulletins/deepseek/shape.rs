//! DeepSeek `/chat/completions` body shaping (整形).
//!
//! Ported from v1 `channels/deepseek.rs`. Both shapers parse the JSON body,
//! mutate it best-effort, and re-serialize — returning the ORIGINAL bytes
//! unchanged on parse failure (request) or when nothing changed (response).

use bytes::Bytes;
use serde_json::{Map, Value};

/// OpenAI chat fields DeepSeek rejects. Stripped from every outbound
/// `/chat/completions` body.
const UNSUPPORTED_CHAT_FIELDS: &[&str] = &[
    "audio",
    "function_call",
    "functions",
    "logit_bias",
    "max_completion_tokens",
    "metadata",
    "modalities",
    "n",
    "parallel_tool_calls",
    "prediction",
    "prompt_cache_key",
    "prompt_cache_retention",
    "reasoning_effort",
    "safety_identifier",
    "seed",
    "service_tier",
    "store",
    "user",
    "verbosity",
    "web_search_options",
];

/// Normalize an outbound `/chat/completions` request body so DeepSeek accepts
/// it. Returns the input unchanged on parse failure (best-effort).
///
/// - Fold `extra_body.thinking` into top-level `thinking` (`adaptive` →
///   `enabled`; DeepSeek only understands `enabled` / `disabled`).
/// - Cap `max_tokens` / `max_completion_tokens` at 8192, then rename
///   `max_completion_tokens` → `max_tokens` when `max_tokens` is absent.
/// - Strip unsupported OpenAI chat fields.
/// - Rewrite `developer` role messages as `system`.
/// - Normalize tools (drop non-`function`, re-emit `{type, function}`) and
///   `tool_choice` (force `"none"` when no tools remain).
pub(super) fn shape_request(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(map) = value.as_object_mut() else {
        return body;
    };

    normalize_extra_body(map);
    cap_and_rename_max_tokens(map);
    for field in UNSUPPORTED_CHAT_FIELDS {
        map.remove(*field);
    }
    normalize_message_roles(map);
    normalize_tools(map);

    match serde_json::to_vec(&value) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

fn cap_and_rename_max_tokens(map: &mut Map<String, Value>) {
    if let Some(max_tokens) = map.get("max_tokens").and_then(Value::as_u64) {
        map.insert("max_tokens".to_string(), Value::from(max_tokens.min(8192)));
    }
    if let Some(max_completion_tokens) = map.get("max_completion_tokens").and_then(Value::as_u64) {
        let capped = max_completion_tokens.min(8192);
        map.insert("max_completion_tokens".to_string(), Value::from(capped));
    }
    if map.get("max_tokens").is_none()
        && let Some(max_completion_tokens) = map.remove("max_completion_tokens")
    {
        map.insert("max_tokens".to_string(), max_completion_tokens);
    }
}

fn normalize_extra_body(map: &mut Map<String, Value>) {
    let Some(extra_body) = map.remove("extra_body") else {
        return;
    };
    if map.contains_key("thinking") {
        return;
    }
    if let Some(thinking) = thinking_from_extra_body(&extra_body) {
        map.insert("thinking".to_string(), thinking);
    }
}

fn thinking_from_extra_body(extra_body: &Value) -> Option<Value> {
    let object = extra_body.as_object()?;
    if let Some(value) = object.get("thinking").and_then(normalize_thinking_value) {
        return Some(value);
    }
    object.get("extra_body").and_then(thinking_from_extra_body)
}

fn normalize_thinking_value(value: &Value) -> Option<Value> {
    let object = value.as_object()?;
    let mode = object.get("type")?.as_str()?;
    let normalized_type = match mode {
        "enabled" => "enabled",
        "disabled" => "disabled",
        // DeepSeek only supports enabled/disabled; adaptive collapses to enabled.
        "adaptive" => "enabled",
        _ => return None,
    };
    Some(serde_json::json!({ "type": normalized_type }))
}

fn normalize_message_roles(map: &mut Map<String, Value>) {
    let Some(messages) = map.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        if let Some(object) = message.as_object_mut() {
            let is_developer = object
                .get("role")
                .and_then(Value::as_str)
                .is_some_and(|role| role.eq_ignore_ascii_case("developer"));
            if is_developer {
                object.insert("role".to_string(), Value::String("system".to_string()));
            }
        }
    }
}

fn normalize_tools(map: &mut Map<String, Value>) {
    if let Some(Value::Array(tools)) = map.remove("tools") {
        let normalized: Vec<Value> = tools.into_iter().filter_map(normalize_tool).collect();
        if !normalized.is_empty() {
            map.insert("tools".to_string(), Value::Array(normalized));
        }
    }

    if let Some(tool_choice) = map.remove("tool_choice")
        && let Some(normalized) = normalize_tool_choice(tool_choice)
    {
        let has_tools = map
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty());
        let normalized = if has_tools || normalized == Value::String("none".to_string()) {
            normalized
        } else {
            Value::String("none".to_string())
        };
        map.insert("tool_choice".to_string(), normalized);
    }
}

fn normalize_tool(tool: Value) -> Option<Value> {
    let mut tool = tool.as_object()?.clone();
    if tool.remove("type")?.as_str()? != "function" {
        return None;
    }
    let function = tool.remove("function")?.as_object()?.clone();
    Some(serde_json::json!({ "type": "function", "function": function }))
}

fn normalize_tool_choice(choice: Value) -> Option<Value> {
    match choice {
        Value::String(mode) => match mode.as_str() {
            "none" | "auto" | "required" => Some(Value::String(mode)),
            _ => None,
        },
        Value::Object(mut object) => {
            if object.remove("type")?.as_str()? != "function" {
                return None;
            }
            let function = object.remove("function")?.as_object()?.clone();
            let name = function.get("name")?.as_str()?.to_string();
            Some(serde_json::json!({
                "type": "function",
                "function": { "name": name }
            }))
        }
        _ => None,
    }
}

/// Normalize a DeepSeek `/chat/completions` response body into the shape
/// downstream OpenAI consumers expect. Returns the input unchanged on parse
/// failure or when nothing actually changed (saves a needless re-serialize).
///
/// - Rewrite `finish_reason: "insufficient_system_resource"` as `"length"`.
/// - Mirror `usage.prompt_cache_hit_tokens` into
///   `usage.prompt_tokens_details.cached_tokens`.
pub(super) fn shape_response(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(map) = value.as_object_mut() else {
        return body;
    };
    let mut changed = false;

    if let Some(choices) = map.get_mut("choices").and_then(Value::as_array_mut) {
        for choice in choices {
            if let Some(choice_map) = choice.as_object_mut()
                && let Some(reason) = choice_map.get_mut("finish_reason")
                && reason.as_str() == Some("insufficient_system_resource")
            {
                *reason = Value::String("length".to_string());
                changed = true;
            }
        }
    }

    if let Some(usage) = map.get_mut("usage").and_then(Value::as_object_mut)
        && let Some(cache_hit_tokens) = usage.get("prompt_cache_hit_tokens").and_then(Value::as_u64)
    {
        let details_value = usage
            .entry("prompt_tokens_details".to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !details_value.is_object() {
            *details_value = Value::Object(Map::new());
        }
        if let Some(details) = details_value.as_object_mut() {
            details
                .entry("cached_tokens".to_string())
                .or_insert(Value::from(cache_hit_tokens));
            changed = true;
        }
    }

    if changed {
        match serde_json::to_vec(&value) {
            Ok(bytes) => Bytes::from(bytes),
            Err(_) => body,
        }
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(value: Value) -> Value {
        let out = shape_request(Bytes::from(serde_json::to_vec(&value).unwrap()));
        serde_json::from_slice(&out).unwrap()
    }

    #[test]
    fn maps_max_completion_tokens_and_developer_role() {
        let body = req(json!({
            "model": "deepseek-chat",
            "max_completion_tokens": 1234,
            "messages": [
                { "role": "developer", "content": "rule" },
                { "role": "user", "content": "hi" }
            ],
            "parallel_tool_calls": true,
            "store": true
        }));
        assert_eq!(body.get("max_tokens").and_then(Value::as_u64), Some(1234));
        assert!(body.get("max_completion_tokens").is_none());
        assert!(body.get("parallel_tool_calls").is_none());
        assert!(body.get("store").is_none());
        assert_eq!(
            body["messages"][0]["role"].as_str(),
            Some("system"),
            "developer role rewritten to system"
        );
    }

    #[test]
    fn caps_max_tokens_at_8192() {
        let body = req(json!({
            "model": "deepseek-chat",
            "max_tokens": 20000,
            "messages": [{ "role": "user", "content": "hi" }]
        }));
        assert_eq!(body.get("max_tokens").and_then(Value::as_u64), Some(8192));
    }

    #[test]
    fn flattens_extra_body_thinking_adaptive_to_enabled() {
        let body = req(json!({
            "model": "deepseek-reasoner",
            "messages": [{ "role": "user", "content": "hi" }],
            "extra_body": { "thinking": { "type": "adaptive" } }
        }));
        assert!(body.get("extra_body").is_none());
        assert_eq!(body["thinking"]["type"].as_str(), Some("enabled"));
    }

    #[test]
    fn drops_non_function_tools_and_forces_tool_choice_none_when_empty() {
        let body = req(json!({
            "model": "deepseek-chat",
            "messages": [{ "role": "user", "content": "hi" }],
            "tools": [{ "type": "retrieval", "retrieval": {} }],
            "tool_choice": "auto"
        }));
        assert!(body.get("tools").is_none());
        assert_eq!(
            body.get("tool_choice").and_then(Value::as_str),
            Some("none")
        );
    }

    #[test]
    fn request_passes_through_invalid_json() {
        let raw = Bytes::from_static(b"not json");
        assert_eq!(shape_request(raw.clone()), raw);
    }

    #[test]
    fn response_maps_finish_reason_and_cache_tokens() {
        let out = shape_response(Bytes::from(
            serde_json::to_vec(&json!({
                "choices": [{
                    "index": 0,
                    "finish_reason": "insufficient_system_resource",
                    "message": { "role": "assistant", "content": "x" }
                }],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "total_tokens": 15,
                    "prompt_cache_hit_tokens": 3
                }
            }))
            .unwrap(),
        ));
        let value: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(
            value["choices"][0]["finish_reason"].as_str(),
            Some("length")
        );
        assert_eq!(
            value["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64(),
            Some(3)
        );
    }

    #[test]
    fn response_unchanged_returns_input_bytes() {
        let raw = Bytes::from_static(br#"{"choices":[{"finish_reason":"stop"}]}"#);
        assert_eq!(shape_response(raw.clone()), raw);
    }
}
