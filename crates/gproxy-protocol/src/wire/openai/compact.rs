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
