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
    pub object: ModelObjectType,
    // Optional because OpenAI-compatible providers (DeepSeek among them)
    // omit it; decoding a model list must not fail on their behalf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
