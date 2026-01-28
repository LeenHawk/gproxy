use serde::{Deserialize, Serialize};

use crate::claude::types::AnthropicHeaders;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPath {
    pub model_id: String,
}

pub type GetModelHeaders = AnthropicHeaders;

#[derive(Debug, Clone)]
pub struct GetModelRequest {
    pub path: GetModelPath,
    pub headers: GetModelHeaders,
}
