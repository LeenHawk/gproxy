//! Hardcoded model catalog exposed via `ModelList` / `ModelGet`.
//!
//! chatgpt.com doesn't serve `/v1/models`; the client-side bundle ships
//! the picker. The catalog is maintained in `models.json` (compiled in
//! via `include_str!`) so operators can update the list without
//! touching Rust.
//!
//! The list is a **fallback** when the dynamic upstream picker
//! (`/backend-api/models/gpts`) is unreachable. The live picker is
//! always preferred — slugs there vary by account plan / version /
//! A/B group.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{Value, json};

const CATALOG_JSON: &str = include_str!("models.json");

#[derive(Debug, Deserialize)]
struct Catalog {
    default_model: String,
    models: Vec<String>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(CATALOG_JSON).expect("invalid built-in chatgpt models.json")
    })
}

fn catalog_ids() -> &'static [&'static str] {
    static IDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    IDS.get_or_init(|| catalog().models.iter().map(String::as_str).collect())
}

/// The id used when none is provided — also the first entry in the
/// list response and the one `prepare_request` resolves unknown model
/// names to.
pub fn default_model() -> &'static str {
    catalog().default_model.as_str()
}

/// Returns the full list of model ids this channel reports.
pub fn known_model_ids() -> &'static [&'static str] {
    catalog_ids()
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
        assert!(ids.contains(&"gpt-5-5"));
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

    #[test]
    fn default_model_is_defined() {
        assert!(!default_model().is_empty());
        assert!(known_model_ids().contains(&default_model()));
    }
}
