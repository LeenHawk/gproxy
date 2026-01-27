use serde::{Deserialize, Serialize};

use crate::openai::get_model::types::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ListObjectType {
    #[serde(rename = "list")]
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ListModelsResponse {
    /// The object type, which is always "list".
    pub object: ListObjectType,
    /// The list of model objects.
    pub data: Vec<Model>,
}
