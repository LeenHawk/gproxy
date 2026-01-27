use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelPath {
    /// Format: models/{model}. It takes the form models/{model}.
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct GetModelRequest {
    pub path: GetModelPath,
}
