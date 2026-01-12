use std::collections::HashMap;

use axum::http::{HeaderMap, StatusCode};
use thiserror::Error;
use time::OffsetDateTime;

use crate::formats::{claude, gemini, openai};

pub type QueryMap = HashMap<String, String>;

#[derive(Debug, Clone)]
pub struct RequestParts<T> {
    pub path: String,
    pub query: QueryMap,
    pub headers: HeaderMap,
    pub body: T,
}

#[derive(Debug, Clone)]
pub struct ResponseParts<T> {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: T,
}

#[derive(Debug, Error)]
pub enum TransformError {
    #[error("missing required field: {0}")]
    Missing(&'static str),
    #[error("unsupported feature: {0}")]
    Unsupported(&'static str),
    #[error("invalid value: {0}")]
    Invalid(&'static str),
}

const OPENAI_MODELS_PATH: &str = "/v1/models";
const CLAUDE_MODELS_PATH: &str = "/v1/models";
const GEMINI_MODELS_PATH: &str = "/v1/models";
const GEMINI_MODELS_PATH_BETA: &str = "/v1beta/models";

fn ensure_path_eq(path: &str, expected: &str) -> Result<(), TransformError> {
    if path.trim_end_matches('/') == expected {
        Ok(())
    } else {
        Err(TransformError::Invalid("models list path"))
    }
}

fn ensure_empty_query(query: &QueryMap) -> Result<(), TransformError> {
    if query.is_empty() {
        Ok(())
    } else {
        Err(TransformError::Unsupported("query parameters"))
    }
}

fn extract_path_suffix(path: &str, prefix: &str) -> Result<String, TransformError> {
    let prefix = prefix.trim_end_matches('/');
    let path = path.trim_end_matches('/');
    let value = path
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .ok_or(TransformError::Invalid("models get path"))?;
    if value.is_empty() {
        return Err(TransformError::Missing("model id"));
    }
    Ok(value.to_string())
}

fn extract_gemini_name(path: &str) -> Result<String, TransformError> {
    if let Ok(name) = extract_path_suffix(path, GEMINI_MODELS_PATH) {
        return Ok(name);
    }
    extract_path_suffix(path, GEMINI_MODELS_PATH_BETA)
}

fn normalize_gemini_name(name: &str) -> String {
    let name = name.trim_start_matches('/');
    if let Some(stripped) = name.strip_prefix("publishers/google/") {
        return stripped.to_string();
    }
    if name.starts_with("models/") {
        return name.to_string();
    }
    format!("models/{name}")
}

fn openai_id_from_gemini(name: &str) -> String {
    normalize_gemini_name(name)
        .strip_prefix("models/")
        .unwrap_or(name)
        .to_string()
}

fn openai_model_from_claude(model: &claude::types::BetaModelInfo) -> openai::types::Model {
    openai::types::Model {
        id: model.id.clone(),
        created: model.created_at.unix_timestamp(),
        object_type: openai::types::ModelObjectType::Model,
        owned_by: "anthropic".to_string(),
    }
}

fn claude_model_from_openai(model: &openai::types::Model) -> claude::types::BetaModelInfo {
    claude::types::BetaModelInfo {
        id: model.id.clone(),
        created_at: OffsetDateTime::from_unix_timestamp(model.created)
            .unwrap_or(OffsetDateTime::UNIX_EPOCH),
        display_name: model.id.clone(),
        model_type: claude::types::ModelObjectType::Model,
    }
}

fn openai_model_from_gemini(model: &gemini::types::Model) -> openai::types::Model {
    openai::types::Model {
        id: openai_id_from_gemini(&model.name),
        created: 0,
        object_type: openai::types::ModelObjectType::Model,
        owned_by: "google".to_string(),
    }
}

fn gemini_model_from_openai(model: &openai::types::Model) -> gemini::types::Model {
    gemini::types::Model {
        name: normalize_gemini_name(&model.id),
        base_model_id: None,
        version: "unknown".to_string(),
        display_name: Some(model.id.clone()),
        description: None,
        input_token_limit: None,
        output_token_limit: None,
        supported_generation_methods: None,
        thinking: None,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
    }
}

fn claude_model_from_gemini(model: &gemini::types::Model) -> claude::types::BetaModelInfo {
    claude::types::BetaModelInfo {
        id: openai_id_from_gemini(&model.name),
        created_at: OffsetDateTime::UNIX_EPOCH,
        display_name: model
            .display_name
            .clone()
            .unwrap_or_else(|| model.name.clone()),
        model_type: claude::types::ModelObjectType::Model,
    }
}

fn gemini_model_from_claude(model: &claude::types::BetaModelInfo) -> gemini::types::Model {
    gemini::types::Model {
        name: normalize_gemini_name(&model.id),
        base_model_id: None,
        version: "unknown".to_string(),
        display_name: Some(model.display_name.clone()),
        description: None,
        input_token_limit: None,
        output_token_limit: None,
        supported_generation_methods: None,
        thinking: None,
        temperature: None,
        max_temperature: None,
        top_p: None,
        top_k: None,
    }
}

fn map_status_headers<T>(
    status: StatusCode,
    headers: &HeaderMap,
    body: T,
) -> Result<ResponseParts<T>, TransformError> {
    if !status.is_success() {
        return Err(TransformError::Unsupported("non-success status"));
    }
    Ok(ResponseParts {
        status,
        headers: headers.clone(),
        body,
    })
}

pub mod models_openai_to_claude;
pub mod models_openai_to_gemini;
pub mod models_claude_to_openai;
pub mod models_claude_to_gemini;
pub mod models_gemini_to_openai;
pub mod models_gemini_to_claude;
pub mod gen_openai_chat_to_gemini_generate;
pub mod gen_openai_chat_to_gemini_generate_stream;
pub mod gen_claude_messages_to_gemini_generate;
pub mod gen_claude_messages_to_gemini_generate_stream;
pub mod gen_gemini_generate_to_openai_chat;
pub mod gen_gemini_generate_to_openai_chat_stream;
pub mod gen_gemini_generate_to_claude_messages;
pub mod gen_gemini_generate_to_claude_messages_stream;
pub mod gen_claude_messages_to_openai_chat;
pub mod gen_claude_messages_to_openai_chat_stream;
pub mod gen_openai_chat_to_claude_messages;
