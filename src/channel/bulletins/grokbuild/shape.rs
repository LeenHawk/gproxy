//! xAI/Grok Build Responses body hygiene.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD as B64;
use bytes::Bytes;
use serde_json::{Map, Value, json};

const COMPOSER_MODEL_PREFIX: &str = "grok-composer-";

pub(super) fn shape_responses_request(body: Bytes) -> Bytes {
    shape_responses_body(body, false, false)
}

pub(super) fn shape_responses_websocket_request(body: Bytes) -> Bytes {
    shape_responses_body(body, true, true)
}

pub(super) fn shape_image_request(body: Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(map) = value.as_object_mut() else {
        return body;
    };

    for key in ["moderation", "partial_images", "size", "stream"] {
        map.remove(key);
    }

    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body)
}

fn shape_responses_body(
    body: Bytes,
    preserve_previous_response_id: bool,
    websocket: bool,
) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(map) = value.as_object_mut() else {
        return body;
    };

    map.insert("stream".into(), Value::Bool(true));
    if !preserve_previous_response_id {
        map.remove("previous_response_id");
    }
    for key in [
        "metadata",
        "prompt_cache_retention",
        "safety_identifier",
        "stream_options",
    ] {
        map.remove(key);
    }

    normalize_tools(map);
    normalize_tool_choice_for_tools(map);
    sanitize_input_reasoning_items(map);
    remove_encrypted_reasoning_include(map);
    strip_unsupported_reasoning_effort(map);
    ensure_composer_session(map);
    if websocket {
        normalize_websocket_frame(map);
    }

    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body)
}

fn normalize_websocket_frame(map: &mut Map<String, Value>) {
    map.insert("type".into(), Value::String("response.create".into()));
    map.remove("stream");
    map.remove("stream_options");
    map.remove("background");
    map.insert("store".into(), Value::Bool(true));
    if map
        .get("previous_response_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        map.remove("instructions");
    }
}

fn normalize_tools(map: &mut Map<String, Value>) {
    let Some(Value::Array(tools)) = map.get("tools") else {
        return;
    };
    let mut changed = false;
    let mut out = Vec::new();
    for tool in tools {
        if tool_type(tool) == "namespace" {
            changed = true;
            if let Some(Value::Array(nested)) = tool.get("tools") {
                for nested_tool in nested {
                    let (normalized, tool_changed) = normalize_tool(nested_tool);
                    changed |= tool_changed;
                    if let Some(normalized) = normalized {
                        out.push(normalized);
                    }
                }
            }
            continue;
        }
        let (normalized, tool_changed) = normalize_tool(tool);
        changed |= tool_changed;
        if let Some(normalized) = normalized {
            out.push(normalized);
        }
    }
    if changed {
        if out.is_empty() {
            map.remove("tools");
        } else {
            map.insert("tools".into(), Value::Array(out));
        }
    }
}

fn normalize_tool(tool: &Value) -> (Option<Value>, bool) {
    match tool_type(tool).as_str() {
        "tool_search" | "image_generation" => (None, true),
        "custom" if tool.get("name").and_then(Value::as_str) == Some("apply_patch") => (None, true),
        "custom" => {
            let mut tool = tool.clone();
            if let Some(object) = tool.as_object_mut() {
                object.insert("type".into(), Value::String("function".into()));
                object
                    .entry("parameters")
                    .or_insert_with(default_parameters);
            }
            (Some(tool), true)
        }
        "web_search" => {
            let mut tool = tool.clone();
            let changed = tool
                .as_object_mut()
                .is_some_and(|object| object.remove("external_web_access").is_some());
            (Some(tool), changed)
        }
        "function" => {
            let mut tool = tool.clone();
            let mut changed = false;
            if let Some(object) = tool.as_object_mut()
                && !object.contains_key("parameters")
            {
                object.insert("parameters".into(), default_parameters());
                changed = true;
            }
            (Some(tool), changed)
        }
        _ => (Some(tool.clone()), false),
    }
}

fn normalize_tool_choice_for_tools(map: &mut Map<String, Value>) {
    let has_tools = matches!(map.get("tools"), Some(Value::Array(tools)) if !tools.is_empty());
    if has_tools {
        return;
    }
    map.remove("tools");
    map.remove("tool_choice");
    map.remove("parallel_tool_calls");
}

fn sanitize_input_reasoning_items(map: &mut Map<String, Value>) {
    let Some(Value::Array(input)) = map.get_mut("input") else {
        return;
    };
    let mut out = Vec::with_capacity(input.len());
    for mut item in std::mem::take(input) {
        let item_type = item_type(&item);
        if item_type != "reasoning" && item_type != "compaction" {
            out.push(item);
            continue;
        }
        let Some(object) = item.as_object_mut() else {
            out.push(item);
            continue;
        };
        if object.get("content").is_some_and(Value::is_null) {
            object.remove("content");
        }
        let invalid_encrypted = object
            .get("encrypted_content")
            .is_some_and(|value| !valid_grok_encrypted_content(value));
        if invalid_encrypted && item_type == "compaction" {
            continue;
        }
        if invalid_encrypted {
            object.remove("encrypted_content");
        }
        out.push(item);
    }

    let (merged, _) = merge_adjacent_reasoning_summaries(out);
    *input = merged;
}

fn merge_adjacent_reasoning_summaries(items: Vec<Value>) -> (Vec<Value>, bool) {
    let mut changed = false;
    let mut out: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(previous) = out.last_mut()
            && can_merge_reasoning_summary(previous, &item)
            && let Some(Value::Array(current)) = item.get("summary")
            && let Some(Value::Array(previous_summary)) = previous.get_mut("summary")
        {
            previous_summary.extend(current.iter().cloned());
            changed = true;
            continue;
        }
        out.push(item);
    }
    (out, changed)
}

fn can_merge_reasoning_summary(previous: &Value, current: &Value) -> bool {
    item_type(previous) == "reasoning"
        && item_type(current) == "reasoning"
        && matches!(previous.get("summary"), Some(Value::Array(_)))
        && matches!(current.get("summary"), Some(Value::Array(items)) if !items.is_empty())
        && current
            .as_object()
            .is_some_and(|object| object.keys().all(|key| key == "type" || key == "summary"))
}

fn remove_encrypted_reasoning_include(map: &mut Map<String, Value>) {
    let Some(Value::Array(include)) = map.get("include") else {
        return;
    };
    let kept: Vec<Value> = include
        .iter()
        .filter(|item| item.as_str() != Some("reasoning.encrypted_content"))
        .cloned()
        .collect();
    if kept.is_empty() {
        map.remove("include");
    } else if kept.len() != include.len() {
        map.insert("include".into(), Value::Array(kept));
    }
}

fn strip_unsupported_reasoning_effort(map: &mut Map<String, Value>) {
    let model = map.get("model").and_then(Value::as_str).unwrap_or_default();
    if supports_reasoning_effort(model) {
        return;
    }
    let Some(Value::Object(reasoning)) = map.get_mut("reasoning") else {
        return;
    };
    reasoning.remove("effort");
    if reasoning.is_empty() {
        map.remove("reasoning");
    }
}

fn ensure_composer_session(map: &mut Map<String, Value>) {
    if map
        .get("prompt_cache_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some()
    {
        return;
    }
    let model = map.get("model").and_then(Value::as_str).unwrap_or_default();
    if model
        .trim()
        .to_ascii_lowercase()
        .starts_with(COMPOSER_MODEL_PREFIX)
    {
        map.insert(
            "prompt_cache_key".into(),
            Value::String(crate::util::rand::uuid_v4()),
        );
    }
}

fn supports_reasoning_effort(model: &str) -> bool {
    let mut name = model.trim().to_ascii_lowercase();
    if let Some((_, tail)) = name.rsplit_once('/') {
        name = tail.to_owned();
    }
    name.starts_with("grok-3-mini")
        || name.starts_with("grok-4.20-multi-agent")
        || name.starts_with("grok-4.3")
}

fn valid_grok_encrypted_content(value: &Value) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    if raw.is_empty() || raw.trim() != raw || raw.starts_with("gAAAA") || raw.contains('=') {
        return false;
    }
    if !raw
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/'))
    {
        return false;
    }
    B64.decode(raw).is_ok_and(|decoded| decoded.len() >= 50)
}

fn tool_type(tool: &Value) -> String {
    tool.get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn item_type(item: &Value) -> String {
    tool_type(item)
}

fn default_parameters() -> Value {
    json!({ "type": "object", "properties": {} })
}
