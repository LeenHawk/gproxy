use serde::{Deserialize, Serialize};

use crate::claude::types::{AnthropicBetaHeader, AnthropicVersion};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPath {
    pub model_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetModelHeaders {
    #[serde(rename = "anthropic-version")]
    pub anthropic_version: AnthropicVersion,
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<AnthropicBetaHeader>,
}

#[derive(Debug, Clone)]
pub struct GetModelRequest {
    pub path: GetModelPath,
    pub headers: GetModelHeaders,
}
