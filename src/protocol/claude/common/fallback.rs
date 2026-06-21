use serde::{Deserialize, Serialize};

use super::{ClaudeModel, JsonObject, OutputConfig, Speed, ThinkingConfig};

/// One entry of the request-level `fallbacks` chain (beta
/// `server-side-fallback-2026-06-01`): a substitute model tried server-side
/// when the requested model declines for policy reasons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub extra: JsonObject,
}
