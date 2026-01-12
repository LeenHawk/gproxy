use axum::http::StatusCode;
use serde_json::Value;

use crate::formats::gemini::model_get::ModelGetResponse;
use crate::formats::gemini::models_list::ModelsListResponse;
use crate::formats::gemini::types::Model;

pub(crate) fn map_models_list(value: Value) -> Result<ModelsListResponse, StatusCode> {
    let model_names = extract_model_names(&value);
    let models = model_names
        .into_iter()
        .map(|name| Model {
            name,
            base_model_id: None,
            version: "v1".to_string(),
            display_name: None,
            description: None,
            input_token_limit: None,
            output_token_limit: None,
            supported_generation_methods: None,
            thinking: None,
            temperature: None,
            max_temperature: None,
            top_p: None,
            top_k: None,
        })
        .collect();
    Ok(ModelsListResponse {
        models,
        next_page_token: None,
    })
}

pub(crate) fn map_model_get(value: Value, name: &str) -> Result<ModelGetResponse, StatusCode> {
    let model_names = extract_model_names(&value);
    if model_names.iter().any(|item| item == name) {
        return Ok(Model {
            name: name.to_string(),
            base_model_id: None,
            version: "v1".to_string(),
            display_name: None,
            description: None,
            input_token_limit: None,
            output_token_limit: None,
            supported_generation_methods: None,
            thinking: None,
            temperature: None,
            max_temperature: None,
            top_p: None,
            top_k: None,
        });
    }
    Err(StatusCode::NOT_FOUND)
}

fn extract_model_names(value: &Value) -> Vec<String> {
    let mut names = Vec::new();
    let Some(models) = value.get("models") else {
        return names;
    };
    if let Some(obj) = models.as_object() {
        names.extend(obj.keys().cloned());
        return names;
    }
    if let Some(items) = models.as_array() {
        for item in items {
            if let Some(name) = item.as_str() {
                names.push(name.to_string());
            } else if let Some(name) = item.get("name").and_then(|value| value.as_str()) {
                names.push(name.to_string());
            }
        }
    }
    names
}
