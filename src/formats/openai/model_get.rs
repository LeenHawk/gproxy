use serde::{Deserialize, Serialize};

use super::types::Model;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGetRequest {
    pub path: ModelGetPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGetPath {
    pub model: String,
}

pub type ModelGetResponse = Model;
