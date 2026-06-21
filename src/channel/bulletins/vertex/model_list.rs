//! Vertex model-list response normalization to standard Gemini shape.
//!
//! Vertex AI's list endpoint returns `{"publisherModels": [...]}` with model
//! `name` set to the full resource path (`publishers/google/models/<id>`),
//! whereas standard Gemini (AI Studio) returns `{"models": [...]}` with
//! `name: "models/<id>"`. `parse_models` (upstream_models.rs) reads the Gemini
//! shape, so we reshape `publisherModels` → `models` and strip the
//! `publishers/google/` prefix from each name.
//!
//! Best-effort: returns the input unchanged on JSON parse failure.

use bytes::Bytes;
use serde_json::Value;

/// Reshape a Vertex `{"publisherModels": [...]}` body into the canonical Gemini
/// `{"models": [...]}` shape. Already-canonical bodies (those carrying `models`)
/// and unparseable bodies are returned unchanged.
pub(super) fn normalize_vertex_model_list(body: Bytes) -> Bytes {
    let Ok(Value::Object(mut map)) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    // Already in standard Gemini format.
    if map.contains_key("models") {
        return body;
    }
    let models = match map.remove("publisherModels") {
        Some(Value::Array(items)) => items
            .into_iter()
            .map(vertex_publisher_model_to_gemini)
            .collect::<Vec<_>>(),
        Some(item) => vec![vertex_publisher_model_to_gemini(item)],
        None => return body,
    };
    let mut out = serde_json::Map::new();
    out.insert("models".to_string(), Value::Array(models));
    if let Some(token) = map.remove("nextPageToken").filter(|v| !v.is_null()) {
        out.insert("nextPageToken".to_string(), token);
    }
    match serde_json::to_vec(&Value::Object(out)) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

/// Convert one Vertex `publisherModel` object to standard Gemini model format,
/// rewriting `name` from `publishers/google/models/<id>` to `models/<id>` and
/// carrying through the common metadata fields.
fn vertex_publisher_model_to_gemini(value: Value) -> Value {
    let Value::Object(map) = value else {
        return value;
    };
    let raw_name = map
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let model_id = if let Some((_, tail)) = raw_name.rsplit_once("/models/") {
        tail
    } else {
        raw_name.strip_prefix("models/").unwrap_or(raw_name)
    };
    let model_id = if model_id.is_empty() {
        "unknown"
    } else {
        model_id
    };

    let mut out = serde_json::Map::new();
    out.insert(
        "name".to_string(),
        Value::String(format!("models/{model_id}")),
    );
    for key in [
        "baseModelId",
        "version",
        "displayName",
        "description",
        "inputTokenLimit",
        "outputTokenLimit",
        "supportedGenerationMethods",
        "thinking",
        "temperature",
        "maxTemperature",
        "topP",
        "topK",
    ] {
        if let Some(v) = map.get(key).cloned().filter(|v| !v.is_null()) {
            out.insert(key.to_string(), v);
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn publisher_models_to_gemini_models() {
        let body = Bytes::from(
            json!({
                "publisherModels": [
                    {
                        "name": "publishers/google/models/gemini-2.5-pro",
                        "displayName": "Gemini 2.5 Pro"
                    },
                    {"name": "publishers/google/models/gemini-2.0-flash"}
                ],
                "nextPageToken": "tok"
            })
            .to_string(),
        );
        let out = normalize_vertex_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert!(v.get("publisherModels").is_none());
        let models = v["models"].as_array().unwrap();
        assert_eq!(models[0]["name"], "models/gemini-2.5-pro");
        assert_eq!(models[0]["displayName"], "Gemini 2.5 Pro");
        assert_eq!(models[1]["name"], "models/gemini-2.0-flash");
        assert_eq!(v["nextPageToken"], "tok");
    }

    #[test]
    fn already_gemini_shape_passes_through() {
        let body = Bytes::from(json!({"models": [{"name": "models/gemini-2.5-pro"}]}).to_string());
        let out = normalize_vertex_model_list(body.clone());
        assert_eq!(out, body);
    }

    #[test]
    fn returns_original_on_parse_failure() {
        let body = Bytes::from_static(b"not json");
        let out = normalize_vertex_model_list(body.clone());
        assert_eq!(out, body);
    }
}
