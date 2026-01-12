use serde::{Deserialize, Serialize};
use serde_valid::Validate;

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<ModelsListHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<ModelsListQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListHeaders {
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ModelsListQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 1)]
    #[validate(maximum = 1000)]
    pub limit: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponse {
    pub data: Vec<BetaModelInfo>,
    pub first_id: String,
    pub has_more: bool,
    pub last_id: String,
}
