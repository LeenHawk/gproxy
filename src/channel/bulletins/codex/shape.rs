//! Codex model-list / model-get RESPONSE shaping (整形).
//!
//! The codex ChatGPT backend serves its model catalogue as
//! `{"models":[{"slug"|"id":…}, …]}`, which is NOT the OpenAI canonical model
//! shape that `parse_models` (and OpenAI-family clients) expect. These shapers
//! reproject it into the OpenAI form so the channel's declared `OpenAi` family
//! stays canonical:
//!
//! - `ListModels` → `{"object":"list","data":[{"id":…,"object":"model",…}]}`
//! - `GetModel`   → a single `{"id":…,"object":"model",…}` object
//!
//! Ported from v1 `channels/codex.rs` (`normalize_codex_model_*_response`).
//! Best-effort: the ORIGINAL bytes are returned unchanged on parse failure or
//! when the body is not in the expected codex shape.

use bytes::Bytes;
use serde_json::{Value, json};

/// Reproject one codex model entry (`{"slug"|"id":…}`) into an OpenAI model
/// object. Returns `None` when neither key is a usable string.
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

/// Reshape a codex `ListModels` body `{"models":[…]}` into the OpenAI list
/// envelope `{"object":"list","data":[…]}`. Returns input unchanged on parse
/// failure or when there is no `models` array.
pub(super) fn shape_model_list(body: Bytes) -> Bytes {
    let Ok(value) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(models) = value.get("models").and_then(Value::as_array) else {
        return body;
    };

    let data: Vec<Value> = models.iter().filter_map(normalize_entry).collect();
    match serde_json::to_vec(&json!({ "object": "list", "data": data })) {
        Ok(out) => Bytes::from(out),
        Err(_) => body,
    }
}

/// Reshape a codex `GetModel` body into a single OpenAI model object. The codex
/// backend has no single-model endpoint, so the body may arrive either as a
/// bare entry (`{"slug"|"id":…}`) or as the `{"models":[…]}` list — in the
/// latter case the first entry is taken. Returns input unchanged otherwise.
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
    match serde_json::to_vec(&model) {
        Ok(out) => Bytes::from(out),
        Err(_) => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_codex_models_to_openai_data() {
        // codex `{models:[{slug}, {id}]}` → OpenAI `{object:list, data:[{id}]}`
        // so `parse_models` can read `data[].id`.
        let body = Bytes::from_static(
            br#"{"models":[{"slug":"gpt-5.4-codex"},{"id":"gpt-5.4"},{"name":"no-id"}]}"#,
        );
        let out = shape_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();

        assert_eq!(v["object"], "list");
        let data = v["data"].as_array().unwrap();
        // The entry lacking both slug/id is dropped.
        assert_eq!(data.len(), 2);
        assert_eq!(data[0]["id"], "gpt-5.4-codex");
        assert_eq!(data[0]["object"], "model");
        assert_eq!(data[0]["owned_by"], "openai");
        assert_eq!(data[1]["id"], "gpt-5.4");
    }

    #[test]
    fn list_passthrough_on_non_codex_shape() {
        // Already-canonical OpenAI body has no `models` array → unchanged.
        let body = Bytes::from_static(br#"{"object":"list","data":[{"id":"gpt-5.4"}]}"#);
        let out = shape_model_list(body.clone());
        assert_eq!(out, body);

        // Garbage / non-JSON → returned verbatim.
        let bad = Bytes::from_static(b"not json");
        assert_eq!(shape_model_list(bad.clone()), bad);
    }

    #[test]
    fn get_codex_model_to_single_object() {
        // List form → first entry as a single OpenAI model object.
        let body = Bytes::from_static(br#"{"models":[{"slug":"gpt-5.4-codex"}]}"#);
        let v: Value = serde_json::from_slice(&shape_model_get(body)).unwrap();
        assert_eq!(v["id"], "gpt-5.4-codex");
        assert_eq!(v["object"], "model");

        // Bare entry → reshaped directly.
        let bare = Bytes::from_static(br#"{"id":"gpt-5.4"}"#);
        let v: Value = serde_json::from_slice(&shape_model_get(bare)).unwrap();
        assert_eq!(v["id"], "gpt-5.4");
        assert_eq!(v["object"], "model");
    }
}
