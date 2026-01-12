use serde::{Deserialize, Serialize};

use super::types::Model;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGetRequest {
    pub path: ModelGetPath,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelGetPath {
    pub name: String,
}

pub type ModelGetResponse = Model;
