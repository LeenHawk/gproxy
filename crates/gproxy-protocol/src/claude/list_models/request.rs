use serde::{Deserialize, Serialize};

use crate::claude::types::AnthropicHeaders;

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

pub type ListModelsHeaders = AnthropicHeaders;

#[derive(Debug, Clone, Default)]
pub struct ListModelsRequest {
    pub query: ListModelsQuery,
    pub headers: ListModelsHeaders,
}
