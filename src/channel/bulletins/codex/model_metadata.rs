//! Codex model metadata request version and OpenAI-compatible response shaping.

use bytes::Bytes;
use serde_json::{Value, json};

pub(super) const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub(super) const CODEX_VERSION: &str = "0.144.0";

fn normalize_entry(model: &Value) -> Option<Value> {
    let id = model
        .get("slug")
        .or_else(|| model.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    Some(json!({
        "id": id,
        "created": 0,
        "object": "model",
        "owned_by": "openai",
    }))
}

pub(super) fn shape_model_list(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(models) = value.get("models").and_then(Value::as_array) else {
        return body;
    };
    let data: Vec<Value> = models.iter().filter_map(normalize_entry).collect();
    serde_json::to_vec(&json!({ "object": "list", "data": data }))
        .map(Bytes::from)
        .unwrap_or(body)
}

pub(super) fn shape_model_get(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let entry = normalize_entry(&value).or_else(|| {
        value
            .get("models")
            .and_then(Value::as_array)
            .and_then(|models| models.iter().find_map(normalize_entry))
    });
    let Some(model) = entry else {
        return body;
    };
    serde_json::to_vec(&model).map(Bytes::from).unwrap_or(body)
}
