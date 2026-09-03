use serde::{Deserialize, Serialize};

use super::{ClaudeModel, JsonObject, OutputConfig, Speed, ThinkingConfig};

/// Request-level fallback routing: either Anthropic's category-aware defaults
/// or an ordered chain of up to three caller-selected models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FallbacksParam {
    Default(FallbacksDefault),
    Models(Vec<FallbackParam>),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FallbacksDefault {
    #[serde(rename = "default")]
    Default,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FallbackCreditTokenParam {
    Token(String),
    Config(FallbackCreditTokenConfig),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FallbackCreditTokenConfig {
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<FallbackCreditMode>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum FallbackCreditMode {
    Known(FallbackCreditModeKnown),
    Unknown(String),
}

// Manual Deserialize: unknown values fall back without formatting an
// unknown-variant error (see `protocol::extensible`).
impl<'de> serde::Deserialize<'de> for FallbackCreditMode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        crate::claude::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FallbackCreditModeKnown {
    #[serde(rename = "strict")]
    Strict,
    #[serde(rename = "best_effort")]
    BestEffort,
}

/// One ordered entry of the request-level `fallbacks` chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct FallbackParam {
    pub model: ClaudeModel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<Speed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
