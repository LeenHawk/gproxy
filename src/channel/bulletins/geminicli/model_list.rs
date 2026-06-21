//! Gemini CLI model-list: bespoke `retrieveUserQuota` endpoint, not the standard
//! Gemini `/v1beta/models` LIST.
//!
//! Code Assist has no model-list endpoint; the official CLI derives the set of
//! usable models from the per-credential quota response. The model-pull hits
//! `/v1beta/models` (GET); [`prepare`](super::GeminiCliChannel::prepare) detects
//! that and instead issues `POST /v1internal:retrieveUserQuota` with
//! `{"project":<id>}` (reusing [`crate::channel::envelope::user_quota_request`]).
//! The quota response carries a `buckets` array; [`quota_to_model_list`] extracts
//! the unique model ids (REQUESTS token type) and reshapes them into the
//! canonical Gemini `{"models":[{name:"models/<id>", ...}]}` shape that
//! `parse_models` (upstream_models.rs) reads.
//!
//! Ported faithfully from v1 `geminicli::quota_to_model_list_response`
//! (`models_from_quota_buckets`). Best-effort: returns the input unchanged on
//! JSON parse failure.

use std::collections::HashSet;

use bytes::Bytes;
use serde_json::{Value, json};

/// Whether this prepare call is the model-list pull: the Gemini family model-pull
/// issues `GET /v1beta/models` (no model id, no `:verb`). Detect on method + the
/// trailing `/models` path segment so the content path stays untouched.
pub(super) fn is_list_models(method: &http::Method, path: &str) -> bool {
    method == http::Method::GET && path.trim_end_matches('/').ends_with("/models")
}

/// Extract unique models from the `buckets` array of a `retrieveUserQuota`
/// response, keeping only the `REQUESTS` token type (one bucket per model), and
/// emit them as canonical Gemini model objects (`name: "models/<id>"`).
fn models_from_quota_buckets(payload: &Value) -> Vec<Value> {
    let Some(buckets) = payload.get("buckets").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for bucket in buckets {
        if let Some(token_type) = bucket.get("tokenType").and_then(Value::as_str)
            && token_type != "REQUESTS"
        {
            continue;
        }
        let Some(model_id_raw) = bucket.get("modelId").and_then(Value::as_str) else {
            continue;
        };
        let model_id = model_id_raw.trim().to_string();
        if model_id.is_empty() || !seen.insert(model_id.clone()) {
            continue;
        }
        let model_name = if model_id.starts_with("models/") {
            model_id.clone()
        } else {
            format!("models/{model_id}")
        };
        models.push(json!({
            "name": model_name,
            "baseModelId": model_id,
            "displayName": model_id,
            "description": "Derived from Gemini CLI retrieveUserQuota buckets.",
            "supportedGenerationMethods": [
                "generateContent",
                "streamGenerateContent",
                "countTokens"
            ]
        }));
    }
    models
}

/// Transform a `retrieveUserQuota` response body into a canonical Gemini model
/// list (`{"models":[…]}`). Returns the input unchanged on JSON parse failure.
pub(super) fn quota_to_model_list(body: Bytes) -> Bytes {
    let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let models = models_from_quota_buckets(&payload);
    match serde_json::to_vec(&json!({ "models": models })) {
        Ok(bytes) => Bytes::from(bytes),
        Err(_) => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_list_models_request() {
        assert!(is_list_models(&http::Method::GET, "/v1beta/models"));
        assert!(is_list_models(&http::Method::GET, "/v1beta/models/"));
        // Content paths carry a `:verb`, never end in `/models`.
        assert!(!is_list_models(
            &http::Method::POST,
            "/v1beta/models/gemini-2.5-pro:generateContent"
        ));
        assert!(!is_list_models(&http::Method::POST, "/v1beta/models"));
        assert!(!is_list_models(
            &http::Method::GET,
            "/v1beta/models/gemini-2.5-pro"
        ));
    }

    #[test]
    fn quota_buckets_to_gemini_models() {
        // Sample v1-shape retrieveUserQuota response: REQUESTS buckets carry the
        // usable model ids; a duplicate and a non-REQUESTS bucket are dropped.
        let body = Bytes::from(
            json!({
                "buckets": [
                    {"modelId": "gemini-2.5-pro", "tokenType": "REQUESTS"},
                    {"modelId": "gemini-2.5-pro", "tokenType": "REQUESTS"},
                    {"modelId": "gemini-2.5-flash", "tokenType": "REQUESTS"},
                    {"modelId": "gemini-2.5-pro", "tokenType": "INPUT"}
                ]
            })
            .to_string(),
        );
        let out = quota_to_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        let models = v["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["name"], "models/gemini-2.5-pro");
        assert_eq!(models[0]["baseModelId"], "gemini-2.5-pro");
        assert_eq!(models[1]["name"], "models/gemini-2.5-flash");
    }

    #[test]
    fn no_buckets_yields_empty_models() {
        let body = Bytes::from(json!({}).to_string());
        let out = quota_to_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["models"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn returns_original_on_parse_failure() {
        let body = Bytes::from_static(b"not json");
        let out = quota_to_model_list(body.clone());
        assert_eq!(out, body);
    }
}
