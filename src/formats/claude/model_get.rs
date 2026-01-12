use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGetRequest {
    pub path: ModelGetPath,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<ModelGetHeaders>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGetPath {
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGetHeaders {
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

pub type ModelGetResponse = BetaModelInfo;
