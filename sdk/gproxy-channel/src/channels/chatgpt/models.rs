//! Hardcoded model catalog exposed via `ModelList` / `ModelGet`.
//!
//! chatgpt.com doesn't serve `/v1/models`; the client-side bundle ships
//! the picker. This module provides a curated list based on the model
//! slugs we've seen in the bundle
//! (`target/samples/bundle_main.min.js`) plus the image models the
//! `/f/conversation` API routes to.
//!
//! The list is static at compile time; operators can override via
//! gproxy's alias table if they want to expose different names.

use serde_json::{Value, json};

/// The id that will be used when none is provided — also the first entry
/// in the list response and the one `prepare_request` resolves unknown
/// model names to.
pub const DEFAULT_MODEL: &str = "gpt-5-4";

/// Returns the full list of model ids this channel reports.
///
/// Used as a **fallback** when the dynamic upstream picker
/// (`/backend-api/models/gpts`) is unreachable. The live picker is
/// always preferred — slugs there vary by account plan / version /
/// A/B group. This list mirrors what a current Team account sees.
pub fn known_model_ids() -> &'static [&'static str] {
    &[
        // gpt-5 family — observed slugs in /backend-api/models/gpts
        "gpt-5-2-instant",
        "gpt-5-2-pro",
        "gpt-5-2-thinking",
        "gpt-5-3",
        "gpt-5-3-instant",
        "gpt-5-4",
        "gpt-5-4-pro",
        "gpt-5-4-thinking",
        // Reasoning model
        "o3",
        // Image models — routed to /f/conversation but not in the
        // editor's models_list, so we hardcode them here too.
        "gpt-image-1",
        "gpt-image-1-mini",
        "gpt-image-1.5",
    ]
}

/// Build an OpenAI-compatible `GET /v1/models` response body.
pub fn openai_model_list_body() -> Vec<u8> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let data: Vec<Value> = known_model_ids()
        .iter()
        .map(|id| {
            json!({
                "id": *id,
                "object": "model",
                "created": now,
                "owned_by": "openai",
            })
        })
        .collect();
    let response = json!({
        "object": "list",
        "data": data,
    });
    serde_json::to_vec(&response).unwrap_or_default()
}

/// Build an OpenAI-compatible `GET /v1/models/:id` response body, or
/// `None` if the model id is not in our catalog.
pub fn openai_model_get_body(id: &str) -> Option<Vec<u8>> {
    known_model_ids().iter().find(|m| **m == id)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let response = json!({
        "id": id,
        "object": "model",
        "created": now,
        "owned_by": "openai",
    });
    Some(serde_json::to_vec(&response).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_contains_defaults_and_image_models() {
        let body = openai_model_list_body();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["object"], "list");
        let ids: Vec<&str> = v["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"gpt-5-3"));
        assert!(ids.contains(&"gpt-5-4-thinking"));
        assert!(ids.contains(&"gpt-image-1"));
        assert!(ids.contains(&"o3"));
    }

    #[test]
    fn get_known_model_returns_body() {
        let body = openai_model_get_body("gpt-5-4-thinking").unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["id"], "gpt-5-4-thinking");
        assert_eq!(v["object"], "model");
    }

    #[test]
    fn get_unknown_model_returns_none() {
        assert!(openai_model_get_body("gpt-made-up").is_none());
    }
}
