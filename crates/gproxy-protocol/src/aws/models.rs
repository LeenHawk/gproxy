use serde::{Deserialize, Serialize};

use super::{
    CustomizationType, FoundationModelLifecycleStatus, InferenceType, ModelModality, Rest,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListFoundationModelsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_customization_type: Option<CustomizationType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_inference_type: Option<InferenceType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_output_modality: Option<ModelModality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_provider: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListFoundationModelsResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_summaries: Option<Vec<FoundationModelSummary>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GetFoundationModelRequest {
    pub model_identifier: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GetFoundationModelResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_details: Option<FoundationModelDetails>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FoundationModelSummary {
    pub model_arn: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<ModelModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<ModelModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_streaming_supported: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customizations_supported: Option<Vec<CustomizationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_types_supported: Option<Vec<InferenceType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_lifecycle: Option<FoundationModelLifecycle>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type FoundationModelDetails = FoundationModelSummary;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FoundationModelLifecycle {
    pub status: FoundationModelLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_of_life_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_of_life_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_extended_access_time: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
