use serde::{Deserialize, Serialize};

use super::common::{ListObjectType, ModelObjectType, OpenAiModelId, Rest};

pub type ModelsWireModel = super::common::OpenAiWireModel<ListModelsRequest, ModelListResponse>;
pub type ModelRetrieveWireModel = super::common::OpenAiWireModel<RetrieveModelRequest, Model>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ListModelsRequest {
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveModelRequest {
    pub model: OpenAiModelId,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelListResponse {
    pub data: Vec<Model>,
    pub object: ListObjectType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: OpenAiModelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_supported: Option<bool>,
    pub object: ModelObjectType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
