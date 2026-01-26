use serde::{Deserialize, Serialize};

use crate::claude::types::{AnthropicBetaHeader, AnthropicVersion};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListModelsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Defaults to 20; allowed range is 1..=1000.
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListModelsHeaders {
    #[serde(rename = "anthropic-version")]
    pub anthropic_version: AnthropicVersion,
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<AnthropicBetaHeader>,
}

#[derive(Debug, Clone, Default)]
pub struct ListModelsRequest {
    pub query: ListModelsQuery,
    pub headers: ListModelsHeaders,
}
