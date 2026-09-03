use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::OperationKey;

use super::{OpenAiModelId, PromptCacheBreakpointMode, PromptCacheMode, PromptCacheTtl};

pub type Rest = Map<String, Value>;
pub type JsonSchema = Map<String, Value>;
pub type LogitBias = BTreeMap<String, f64>;
pub type Metadata = BTreeMap<String, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PromptCacheOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PromptCacheMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<PromptCacheTtl>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct PromptCacheBreakpoint {
    pub mode: PromptCacheBreakpointMode,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationConfig {
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<ModerationPolicy>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ModerationPolicyRule>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<ModerationPolicyRule>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationPolicyRule {
    pub mode: ModerationPolicyMode,
    #[serde(default, flatten)]
    pub rest: Rest,
}

strict_string_enum!(ModerationPolicyMode {
    Score => "score",
    Block => "block",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationResult {
    pub categories: BTreeMap<String, bool>,
    pub category_applied_input_types: BTreeMap<String, Vec<ModerationInputType>>,
    pub category_scores: BTreeMap<String, f64>,
    pub flagged: bool,
    pub model: OpenAiModelId,
    #[serde(rename = "type")]
    pub type_: ModerationResultType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModerationInputType {
    Text,
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationResultType {
    #[serde(rename = "moderation_result")]
    ModerationResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationError {
    pub code: String,
    pub message: String,
    #[serde(rename = "type")]
    pub type_: ModerationErrorType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModerationErrorType {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, PartialEq, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct OpenAiWireModel<TRequest, TResponse> {
    pub operation_key: OperationKey,
    pub request: Option<TRequest>,
    pub response: Option<TResponse>,
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrList {
    String(String),
    List(Vec<String>),
}
