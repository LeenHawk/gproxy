use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPath {
    /// The model ID to retrieve.
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct GetModelRequest {
    pub path: GetModelPath,
}
