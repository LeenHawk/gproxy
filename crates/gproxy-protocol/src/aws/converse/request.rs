use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::Rest;

use super::{
    GuardrailConfiguration, GuardrailStreamConfiguration, InferenceConfiguration, Message,
    OutputConfig, PerformanceConfiguration, PromptVariables, RequestMetadata, ServiceTier,
    SystemContentBlock, ToolConfiguration,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConverseRequest {
    /// `upstream_docs/aws/docs/Converse.md`, `additionalModelRequestFields`:
    /// model-specific parameters documented as a JSON value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_request_fields: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_response_field_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_config: Option<GuardrailConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_config: Option<InferenceConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance_config: Option<PerformanceConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_variables: Option<PromptVariables>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_metadata: Option<RequestMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<SystemContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfiguration>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConverseStreamRequest {
    /// `upstream_docs/aws/docs/ConverseStream.md`, `additionalModelRequestFields`:
    /// model-specific parameters documented as a JSON value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_request_fields: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_response_field_paths: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_config: Option<GuardrailStreamConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inference_config: Option<InferenceConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance_config: Option<PerformanceConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_variables: Option<PromptVariables>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_metadata: Option<RequestMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<SystemContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<ToolConfiguration>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
