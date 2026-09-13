//! Public Responses compaction body; source: Compact a response.md.
//! Model is required-nullable, input optional-nullable; output is the same 28-item
//! response union, with required usage accounting. Model resources remain open strings.
use super::responses::{input::Input, response::ResponseOutputItem};
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CompactRequestBody {
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_options: Option<Option<super::responses::generate::PromptCacheOptions>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_retention: Option<Option<super::responses::generate::PromptCacheRetention>>,
    #[wire(required)]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input: Option<Option<Input>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_response_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_tier: Option<Option<ServiceTier>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type CompactRequest = crate::WireRequest<CompactRequestBody>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ServiceTier {
    Auto,
    Default,
    Fast,
    Flex,
    Priority,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CompactResponseBody {
    pub usage: super::responses::response::ResponseUsage,
    pub id: String,
    pub created_at: i64,
    pub object: CompactObject,
    pub output: Vec<ResponseOutputItem>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type CompactResponse = crate::WireResponse<CompactResponseBody>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CompactObject {
    #[serde(rename = "response.compaction")]
    ResponseCompaction,
}

/// Codex client compaction request from codex-api CompactionInput. This is
/// distinct from the public /responses/compact resource schema above.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ClientCompactRequestBody {
    pub model: String,
    pub input: Vec<super::guardian::ClientResponseItem>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<super::guardian::Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<super::guardian::ClientTextControls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_programs: Option<super::guardian::AccessPrograms>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
/// The client consumes this complete replacement array verbatim; no invented
/// usage, timestamp, response ID or foreign ciphertext is required.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ClientCompactResponseBody {
    pub output: Vec<super::guardian::ClientResponseItem>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type ClientCompactRequest = crate::WireRequest<ClientCompactRequestBody>;
pub type ClientCompactResponse = crate::WireResponse<ClientCompactResponseBody>;
