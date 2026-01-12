use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;

use super::generate_content::GenerateContentRequest;
use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
#[validate(custom = validate_count_tokens_request)]
pub struct CountTokensRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(min_items = 1)]
    #[validate]
    pub contents: Option<Vec<Content>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "generate_content_request"
    )]
    #[validate]
    pub generate_content_request: Option<GenerateContentRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountTokensResponse {
    pub total_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content_token_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<Vec<ModalityTokenCount>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_tokens_details: Option<Vec<ModalityTokenCount>>,
}

fn validate_count_tokens_request(req: &CountTokensRequest) -> Result<(), ValidationError> {
    match (&req.contents, &req.generate_content_request) {
        (None, None) => Err(ValidationError::Custom("missing gemini prompt".to_string())),
        (Some(_), Some(_)) => Err(ValidationError::Custom(
            "contents and generateContentRequest are mutually exclusive".to_string(),
        )),
        _ => Ok(()),
    }
}
