use serde::{Deserialize, Serialize};

use super::types::Model;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelsListRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsListResponse {
    #[serde(rename = "object")]
    pub object_type: ListObjectType,
    pub data: Vec<Model>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ListObjectType {
    #[serde(rename = "list")]
    List,
}
