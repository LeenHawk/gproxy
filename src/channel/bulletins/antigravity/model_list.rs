//! Antigravity model-list (bespoke `fetchAvailableModels`) request + response.
//!
//! Antigravity does not expose the standard Gemini `/v1beta/models` list
//! endpoint. Instead the desktop client POSTs an empty body to the Code Assist
//! `/v1internal:fetchAvailableModels` endpoint and receives a bespoke payload
//! (`{"models": {...}, "<purpose>_model_ids": [...], ...}`). The admin
//! model-pull issues a `GET /v1beta/models`, so [`prepare`] must redirect that
//! to the bespoke POST, and [`shape_response`] must reshape the bespoke payload
//! into the canonical Gemini `{"models": [{"name": "models/<id>", ...}]}` shape
//! that `parse_models` (upstream_models.rs) reads.
//!
//! Ported faithfully from v1 `channels/antigravity.rs`
//! (`available_models_to_list_response` / `extract_available_models`); these
//! upstreams are untested-without-credentials, so v1 fidelity is the bar.
//!
//! [`prepare`]: super::AntigravityChannel::prepare
//! [`shape_response`]: super::AntigravityChannel::shape_response

use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use serde_json::{Value, json};

/// The bespoke Antigravity model-list path (relative to the Code Assist base).
pub(super) const FETCH_AVAILABLE_MODELS_PATH: &str = "/v1internal:fetchAvailableModels";

/// Detect the admin model-pull's ListModels request. The pull issues
/// `GET /v1beta/models` (no model id, no `:verb`), whereas every content path
/// is a POST carrying a `:generateContent` / `:streamGenerateContent` verb.
pub(super) fn is_list_models_request(method: &http::Method, path: &str) -> bool {
    method == http::Method::GET && path.ends_with("/models") && !path.contains(':')
}

/// Reshape a `fetchAvailableModels` response body into the canonical Gemini
/// model-list shape (`{"models": [{"name": "models/<id>", ...}]}`). Returns the
/// original bytes on JSON parse failure.
pub(super) fn available_models_to_list_response(body: Bytes) -> Bytes {
    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return body,
    };
    let models = extract_available_models(&payload);
    match serde_json::to_vec(&json!({ "models": models })) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

/// Extract models from a `fetchAvailableModels` response.
///
/// The response carries either `{"models": {"model-id": {...}, ...}}` (object)
/// or `{"models": [{"id": "...", ...}, ...]}` (array). Newer Antigravity
/// responses also expose model ids through purpose-specific fields such as
/// `image_generation_model_ids` and `tiered_model_ids`; keep those models
/// visible even when absent from the main `models` map.
fn extract_available_models(payload: &Value) -> Vec<Value> {
    let mut model_meta = BTreeMap::<String, Value>::new();
    let mut model_methods = BTreeMap::<String, BTreeSet<&'static str>>::new();

    if let Some(models_obj) = payload.get("models").and_then(Value::as_object) {
        for (model_id, meta) in models_obj {
            let id = normalize_model_id(model_id);
            model_methods
                .entry(id.clone())
                .or_default()
                .extend(generation_methods());
            model_meta.insert(id, meta.clone());
        }
    } else if let Some(models_arr) = payload.get("models").and_then(Value::as_array) {
        for item in models_arr {
            if let Some(id) = item
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| item.get("name").and_then(Value::as_str))
            {
                let id = normalize_model_id(id);
                model_methods
                    .entry(id.clone())
                    .or_default()
                    .extend(generation_methods());
                model_meta.insert(id, item.clone());
            } else if let Some(value) = item.as_str() {
                add_available_model_id(&mut model_methods, value, generation_methods());
            }
        }
    }

    add_model_ids_from_fields(
        payload,
        &[
            "default_agent_model_id",
            "defaultAgentModelId",
            "agent_model_sorts",
            "agentModelSorts",
            "battle_mode_model_sorts",
            "battleModeModelSorts",
            "command_model_ids",
            "commandModelIds",
            "tab_model_ids",
            "tabModelIds",
            "mquery_model_ids",
            "mqueryModelIds",
            "web_search_model_ids",
            "webSearchModelIds",
            "commit_message_model_ids",
            "commitMessageModelIds",
            "audio_transcription_model_ids",
            "audioTranscriptionModelIds",
            "tiered_model_ids",
            "tieredModelIds",
        ],
        &mut model_methods,
        generation_methods(),
    );
    add_model_ids_from_fields(
        payload,
        &["image_generation_model_ids", "imageGenerationModelIds"],
        &mut model_methods,
        generation_methods(),
    );

    let mut models = model_methods
        .into_iter()
        .filter(|(model_id, _)| !is_embedding_model_id(model_id))
        .map(|(model_id, methods)| {
            let meta = model_meta.get(&model_id).unwrap_or(&Value::Null);
            build_model_entry(&model_id, meta, &methods)
        })
        .collect::<Vec<_>>();

    models.sort_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name.cmp(b_name)
    });
    models.dedup_by(|a, b| {
        let a_name = a.get("name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("name").and_then(Value::as_str).unwrap_or_default();
        a_name == b_name
    });
    models
}

/// Generation methods every non-embedding model advertises (image-generation
/// models share the same set in v1, so a single helper covers both).
fn generation_methods() -> BTreeSet<&'static str> {
    BTreeSet::from(["countTokens", "generateContent", "streamGenerateContent"])
}

fn is_embedding_model_id(model_id: &str) -> bool {
    let lower = model_id.to_ascii_lowercase();
    lower.contains("embedding") || lower.contains("embed")
}

fn add_model_ids_from_fields(
    payload: &Value,
    field_names: &[&str],
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    methods: BTreeSet<&'static str>,
) {
    for field in field_names {
        if let Some(value) = payload.get(*field) {
            add_model_ids_from_value(models, value, &methods);
        }
    }
}

fn add_model_ids_from_value(
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    value: &Value,
    methods: &BTreeSet<&'static str>,
) {
    match value {
        Value::String(model_id) => add_available_model_id(models, model_id, methods.clone()),
        Value::Array(values) => {
            for value in values {
                add_model_ids_from_value(models, value, methods);
            }
        }
        Value::Object(object) => {
            let direct_model_id = object
                .get("model_id")
                .and_then(Value::as_str)
                .or_else(|| object.get("modelId").and_then(Value::as_str))
                .or_else(|| object.get("id").and_then(Value::as_str))
                .or_else(|| object.get("name").and_then(Value::as_str));
            if let Some(model_id) = direct_model_id {
                add_available_model_id(models, model_id, methods.clone());
            } else {
                for value in object.values() {
                    add_model_ids_from_value(models, value, methods);
                }
            }
        }
        _ => {}
    }
}

fn add_available_model_id(
    models: &mut BTreeMap<String, BTreeSet<&'static str>>,
    model_id: &str,
    methods: BTreeSet<&'static str>,
) {
    let model_id = normalize_model_id(model_id);
    if model_id.is_empty() {
        return;
    }
    models.entry(model_id).or_default().extend(methods);
}

fn normalize_model_id(model: &str) -> String {
    model
        .trim()
        .trim_start_matches('/')
        .trim_start_matches("models/")
        .to_string()
}

fn build_model_entry(model_id: &str, meta: &Value, methods: &BTreeSet<&'static str>) -> Value {
    let display_name = meta
        .get("displayName")
        .and_then(Value::as_str)
        .or_else(|| meta.get("display_name").and_then(Value::as_str))
        .unwrap_or(model_id);
    let methods = methods.iter().copied().collect::<Vec<_>>();

    let mut obj = json!({
        "name": format!("models/{model_id}"),
        "baseModelId": model_id,
        "version": "1",
        "displayName": display_name,
        "supportedGenerationMethods": methods
    });

    if let Some(limit) = meta.get("maxTokens").and_then(Value::as_u64) {
        obj["inputTokenLimit"] = json!(limit);
    }
    if let Some(limit) = meta
        .get("maxOutputTokens")
        .and_then(Value::as_u64)
        .or_else(|| meta.get("outputTokenLimit").and_then(Value::as_u64))
    {
        obj["outputTokenLimit"] = json!(limit);
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_list_models_request() {
        assert!(is_list_models_request(&http::Method::GET, "/v1beta/models"));
        // Content paths carry a `:verb` and a model id — not a list request.
        assert!(!is_list_models_request(
            &http::Method::POST,
            "/v1beta/models/gemini-2.5-pro:generateContent"
        ));
        assert!(!is_list_models_request(
            &http::Method::GET,
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent"
        ));
        // A GET model-get path ends with the model id, not `/models`.
        assert!(!is_list_models_request(
            &http::Method::GET,
            "/v1beta/models/gemini-2.5-pro"
        ));
    }

    #[test]
    fn reshapes_fetch_available_models_to_gemini_list() {
        // Sample v1-shaped fetchAvailableModels response: a `models` object plus
        // grouped capability fields and an embedding model that must be dropped.
        let body = Bytes::from(
            json!({
                "models": {
                    "gemini-2.5-pro": {
                        "displayName": "Gemini 2.5 Pro",
                        "maxTokens": 1048576,
                        "maxOutputTokens": 65536
                    }
                },
                "image_generation_model_ids": ["gemini-2.5-flash-image-preview"],
                "embedding_model_ids": ["gemini-embedding-001"],
                "tiered_model_ids": {"high": "gemini-2.5-flash"}
            })
            .to_string(),
        );
        let out = available_models_to_list_response(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        let models = v["models"].as_array().unwrap();
        let names: Vec<&str> = models.iter().map(|m| m["name"].as_str().unwrap()).collect();

        // Canonical Gemini shape: each name is `models/<id>`, sorted, embeddings
        // dropped, grouped-capability + tiered ids surfaced.
        assert_eq!(
            names,
            vec![
                "models/gemini-2.5-flash",
                "models/gemini-2.5-flash-image-preview",
                "models/gemini-2.5-pro",
            ]
        );
        // The `embedding`-bearing id never appears.
        assert!(!names.iter().any(|n| n.contains("embedding")));

        // Metadata carried from the `models` map onto the canonical entry.
        let pro = models
            .iter()
            .find(|m| m["name"] == "models/gemini-2.5-pro")
            .unwrap();
        assert_eq!(pro["displayName"], "Gemini 2.5 Pro");
        assert_eq!(pro["inputTokenLimit"], 1048576);
        assert_eq!(pro["outputTokenLimit"], 65536);
        assert!(
            pro["supportedGenerationMethods"]
                .as_array()
                .unwrap()
                .iter()
                .any(|m| m == "generateContent")
        );
    }

    #[test]
    fn returns_original_on_parse_failure() {
        let body = Bytes::from_static(b"not json");
        let out = available_models_to_list_response(body.clone());
        assert_eq!(out, body);
    }
}
