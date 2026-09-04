//! Codex model metadata request version and OpenAI-compatible response shaping.

use bytes::Bytes;
use serde_json::{Value, json};

pub(super) const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub(super) const CODEX_VERSION: &str = "0.153.2";

/// Codex reports two windows: `context_window` is the CLI's *default* budget,
/// and `max_context_window` is documented upstream as the "maximum context
/// window allowed for config overrides" — the CLI clamps a user-raised
/// `model_context_window` to it (`models-manager` `model_info.rs`). A proxy
/// imposes no client-side default, so the ceiling is the real capability:
/// GPT-5.6 defaults to 272k but permits 872k. Prefer it, fall back to the
/// default, and drop non-positive values rather than reporting a bogus limit.
fn positive(model: &Value, key: &str) -> Option<i64> {
    model.get(key).and_then(Value::as_i64).filter(|v| *v > 0)
}

fn normalize_entry(model: &Value) -> Option<Value> {
    let id = model
        .get("slug")
        .or_else(|| model.get("id"))
        .and_then(Value::as_str)?
        .to_string();
    let mut entry = json!({
        "id": id,
        "created": 0,
        "object": "model",
        "owned_by": "openai",
    });
    let object = entry.as_object_mut()?;
    // Keys below match what `credentials::upstream_models::parse` reads for the
    // OpenAI family, so the catalogue lands in `provider_models` unchanged.
    if let Some(name) = model.get("display_name").and_then(Value::as_str) {
        object.insert("display_name".into(), Value::from(name));
    }
    let ceiling = positive(model, "max_context_window");
    if let Some(window) = ceiling.or_else(|| positive(model, "context_window")) {
        object.insert("context_window".into(), Value::from(window));
    }
    // Kept verbatim alongside the resolved window so the headroom stays visible.
    if let Some(max_window) = ceiling {
        object.insert("max_context_window".into(), Value::from(max_window));
    }
    if let Some(max_output) = positive(model, "max_output_tokens") {
        object.insert("max_output_tokens".into(), Value::from(max_output));
    }
    // Every codex model is a reasoning model; it advertises the efforts it
    // accepts instead of a boolean flag.
    if let Some(levels) = model
        .get("supported_reasoning_levels")
        .and_then(Value::as_array)
    {
        object.insert("thinking_supported".into(), Value::from(!levels.is_empty()));
    }
    Some(entry)
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
