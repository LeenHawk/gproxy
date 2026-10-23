use serde::{Deserialize, Serialize};

use crate::Rest;

/// Request/response shapes for `GET /v1/models` and `GET /v1/models/{model_id}`.
/// `after_id`, `before_id`, and `limit` are query parameters; `anthropic_beta`
/// is the repeated `anthropic-beta` request header; `model_id` is a path
/// parameter. Source: `upstream_docs/claude/docs/List Models.md` and
/// `Get a Model.md`.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ListModelsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsRequest = crate::WireRequest<()>;
pub type GetModelRequest = crate::WireRequest<()>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ListModelsResponseBody {
    pub data: Vec<ModelInfo>,
    #[wire(required)]
    pub first_id: Option<String>,
    pub has_more: bool,
    #[wire(required)]
    pub last_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type ListModelsResponse = crate::WireResponse<ListModelsResponseBody>;
pub type GetModelResponse = crate::WireResponse<ModelInfo>;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    #[wire(required)]
    pub allowed_fallback_models: Option<Vec<String>>,
    #[wire(required)]
    pub capabilities: Option<ModelCapabilities>,
    pub created_at: String,
    pub display_name: String,
    /// Provider-reported model line; new lines are allowed without inferring from IDs.
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub line: Option<Option<String>>,
    #[wire(required)]
    pub max_input_tokens: Option<i64>,
    #[wire(required)]
    pub max_tokens: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<ModelLifecycle>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub deprecated_at: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub retires_at: Option<Option<String>>,
    #[serde(rename = "type")]
    pub type_: ModelType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ModelType {
    Model,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ModelCapabilities {
    pub batch: CapabilitySupport,
    pub citations: CapabilitySupport,
    pub code_execution: CapabilitySupport,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub compaction: Option<Option<CompactionCapability>>,
    pub context_management: ContextManagementCapability,
    pub effort: EffortCapability,
    pub image_input: CapabilitySupport,
    pub pdf_input: CapabilitySupport,
    pub structured_outputs: CapabilitySupport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_tools: Option<ServerToolsCapability>,
    pub thinking: ThinkingCapability,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CapabilitySupport {
    pub supported: bool,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContextManagementCapability {
    #[wire(required)]
    pub clear_thinking_20251015: Option<CapabilitySupport>,
    #[wire(required)]
    pub clear_tool_uses_20250919: Option<CapabilitySupport>,
    #[wire(required)]
    pub compact_20260112: Option<CapabilitySupport>,
    pub supported: bool,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EffortCapability {
    pub high: CapabilitySupport,
    pub low: CapabilitySupport,
    pub max: CapabilitySupport,
    pub medium: CapabilitySupport,
    pub supported: bool,
    #[wire(required)]
    pub xhigh: Option<CapabilitySupport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ThinkingCapability {
    pub supported: bool,
    pub types: ThinkingTypes,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ThinkingTypes {
    pub adaptive: CapabilitySupport,
    pub enabled: CapabilitySupport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<CapabilitySupport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub between_tools: Option<CapabilitySupport>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Server tool availability is separate from programmatic tool calling
/// (`ModelCapabilities::code_execution`).
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ServerToolsCapability {
    pub supported: bool,
    pub web_search: CapabilitySupport,
    pub code_execution: CapabilitySupport,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

fn present_nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// The lifecycle is authoritative; a past scheduled retirement date alone does
/// not mean that a model has retired.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ModelLifecycle {
    Active,
    Deprecated,
    Retired,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CompactionCapability {
    pub supported: bool,
    pub summarize: CapabilitySupport,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
