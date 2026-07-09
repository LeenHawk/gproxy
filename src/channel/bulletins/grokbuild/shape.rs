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
    strip_non_positive_top_p(map);
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
        "web_search"
        | "web_search_2025_08_26"
        | "web_search_preview"
        | "web_search_preview_2025_03_11" => {
            let mut tool = tool.clone();
            let mut changed = false;
            if let Some(object) = tool.as_object_mut() {
                changed |= object
                    .insert("type".into(), Value::String("web_search".into()))
                    .and_then(|previous| previous.as_str().map(|s| s != "web_search"))
                    .unwrap_or(true);
                for key in ["external_web_access", "search_context_size"] {
                    changed |= object.remove(key).is_some();
                }
            }
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
    normalize_search_tool_choice(map);
    let has_tools = matches!(map.get("tools"), Some(Value::Array(tools)) if !tools.is_empty());
    if has_tools {
        return;
    }
    map.remove("tools");
    map.remove("tool_choice");
    map.remove("parallel_tool_calls");
}

fn normalize_search_tool_choice(map: &mut Map<String, Value>) {
    let Some(tool_choice) = map.get_mut("tool_choice") else {
        return;
    };
    if search_tool_choice(tool_choice) {
        map.remove("tool_choice");
    }
}

fn search_tool_choice(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    match object.get("type").and_then(Value::as_str) {
        Some("web_search")
        | Some("web_search_2025_08_26")
        | Some("web_search_preview")
        | Some("web_search_preview_2025_03_11")
        | Some("x_search") => true,
        Some("allowed_tools") => {
            object
                .get("tools")
                .and_then(Value::as_array)
                .is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|tool| search_tool_type(tool_type(tool).as_str()))
                })
        }
        _ => false,
    }
}

fn search_tool_type(tool_type: &str) -> bool {
    matches!(
        tool_type,
        "web_search"
            | "web_search_2025_08_26"
            | "web_search_preview"
            | "web_search_preview_2025_03_11"
            | "x_search"
    )
}

fn strip_non_positive_top_p(map: &mut Map<String, Value>) {
    if map
        .get("top_p")
        .and_then(Value::as_f64)
        .is_some_and(|top_p| top_p <= 0.0)
    {
        map.remove("top_p");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_search_preview_tools_for_xai() {
        let body = Bytes::from(
            json!({
                "model": "grok-4.3",
                "input": "search",
                "tools": [{
                    "type": "web_search_preview_2025_03_11",
                    "search_context_size": "low",
                    "external_web_access": true,
                    "filters": {"allowed_domains": ["example.com"]}
                }, {
                    "type": "x_search",
                    "included_x_handles": ["xai"]
                }],
                "tool_choice": {
                    "type": "allowed_tools",
                    "mode": "required",
                    "tools": [{"type": "x_search"}]
                }
            })
            .to_string(),
        );

        let shaped: Value = serde_json::from_slice(&shape_responses_request(body)).unwrap();
        assert_eq!(shaped["tools"][0]["type"], "web_search");
        assert!(shaped["tools"][0].get("search_context_size").is_none());
        assert!(shaped["tools"][0].get("external_web_access").is_none());
        assert_eq!(
            shaped["tools"][0]["filters"]["allowed_domains"][0],
            "example.com"
        );
        assert_eq!(shaped["tools"][1]["type"], "x_search");
        assert_eq!(shaped["tools"][1]["included_x_handles"][0], "xai");
        assert!(shaped.get("tool_choice").is_none());
    }

    #[test]
    fn strips_non_positive_top_p_for_xai() {
        for top_p in [0.0, -0.1] {
            let body = Bytes::from(
                json!({
                    "model": "grok-4.5",
                    "input": "hello",
                    "top_p": top_p
                })
                .to_string(),
            );

            let shaped: Value = serde_json::from_slice(&shape_responses_request(body)).unwrap();
            assert!(shaped.get("top_p").is_none());
        }
    }

    #[test]
    fn keeps_positive_top_p_for_xai() {
        let body = Bytes::from(
            json!({
                "model": "grok-4.5",
                "input": "hello",
                "top_p": 0.1
            })
            .to_string(),
        );

        let shaped: Value = serde_json::from_slice(&shape_responses_request(body)).unwrap();
        assert_eq!(shaped["top_p"], 0.1);
    }
}
