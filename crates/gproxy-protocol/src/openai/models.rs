use serde::{Deserialize, Serialize};

use super::common::{ListObjectType, ModelObjectType, OpenAiModelId, Rest};

pub type ModelsWireModel = super::common::OpenAiWireModel<ListModelsRequest, ModelListResponse>;
pub type ModelRetrieveWireModel = super::common::OpenAiWireModel<RetrieveModelRequest, Model>;

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Default, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListModelsRequest {
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RetrieveModelRequest {
    pub model: OpenAiModelId,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModelListResponse {
    pub data: Vec<Model>,
    pub object: ListObjectType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Model {
    pub id: OpenAiModelId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    // gproxy extensions: limits and capabilities surfaced from providers that
    // report them (Claude, Gemini, Codex). The official OpenAI model object
    // has none of these fields, so a consumer of this schema must treat them
    // as gproxy-specific rather than OpenAI-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    /// Input allowance. OpenAI has no separate input limit — the context
    /// window *is* the accepted input size — so peers that split the two
    /// (Claude's `max_input_tokens`, Gemini's `inputTokenLimit`) map here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// Codex reports an override ceiling separately from the default budget;
    /// kept verbatim so a client can see the headroom, while `context_window`
    /// carries the value the catalogue consumes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_window: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_supported: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_parameters: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_reasoning_levels: Option<Vec<ModelReasoningLevel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_reasoning_level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tiers: Option<Vec<ModelServiceTier>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_service_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_methods: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supported_actions: Option<Vec<String>>,
    pub object: ModelObjectType,
    // Optional because OpenAI-compatible providers (DeepSeek among them)
    // omit it; decoding a model list must not fail on their behalf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModelReasoningLevel {
    pub effort: String,
    pub description: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModelServiceTier {
    pub id: String,
    pub name: String,
    pub description: String,
}
