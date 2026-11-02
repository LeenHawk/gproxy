//! SDK-owned vendor options. Portable controls live in `Settings`; native
//! controls use protocol's existing field types. `rest` is for unknown fields.
use gproxy_protocol::{
    Rest,
    claude::{count_tokens as cq, generate_content as c, tools as ct},
    gemini as g,
    openai::{
        chat::{request as q, tools as qt},
        responses::{generate as o, input as i, tools as ot},
    },
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProviderOptions {
    #[serde(default)]
    pub claude: ClaudeOptions,
    #[serde(default)]
    pub openai: OpenaiOptions,
    #[serde(default)]
    pub gemini: GeminiOptions,
}
impl ProviderOptions {
    pub fn new() -> Self {
        Self::default()
    }
    #[cfg(any(
        all(
            feature = "count-tokens",
            any(feature = "claude", feature = "openai", feature = "gemini")
        ),
        feature = "embeddings",
        feature = "rerank",
        feature = "audio",
        feature = "images",
        feature = "video"
    ))]
    pub(crate) fn remainder(&self, vendor: &crate::Vendor) -> crate::Result<&Rest> {
        match vendor {
            crate::Vendor::Claude => Ok(&self.claude.rest),
            crate::Vendor::Openai => Ok(&self.openai.rest),
            crate::Vendor::Gemini => Ok(&self.gemini.rest),
            crate::Vendor::Unknown(_) => Err(crate::Error::invalid(
                "unknown vendor options cannot be sent",
            )),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ClaudeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub thinking: Option<cq::ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cache_control: Option<gproxy_protocol::claude::content::CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub container: Option<c::ContainerParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub context_management: Option<cq::ContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub output_config: Option<cq::OutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub service_tier: Option<c::ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub speed: Option<cq::Speed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tools: Option<Vec<ct::ToolUnion>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiOptions {
    #[serde(default)]
    pub responses: ResponsesOptions,
    #[serde(default)]
    pub chat: ChatOptions,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ResponsesOptions {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reasoning: Option<Option<i::ReasoningConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub include: Option<Option<Vec<o::ResponseIncludable>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub store: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub background: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parallel_tool_calls: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub previous_response_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_options: Option<o::PromptCacheOptions>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_retention: Option<Option<o::PromptCacheRetention>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub service_tier: Option<Option<o::ServiceTier>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub text: Option<i::TextConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tools: Option<Vec<ot::Tool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub truncation: Option<Option<o::GenerateTruncation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChatOptions {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub stream_options: Option<Option<q::StreamOptions>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub audio: Option<Option<q::ChatAudioParam>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub reasoning_effort: Option<Option<q::ReasoningEffort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub response_format: Option<q::ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tools: Option<Vec<qt::ChatTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub parallel_tool_calls: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub seed: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub store: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_options: Option<q::PromptCacheOptions>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub prompt_cache_retention: Option<Option<q::PromptCacheRetention>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub service_tier: Option<Option<q::ServiceTier>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub web_search_options: Option<q::WebSearchOptions>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub generation_config: Option<g::GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tools: Option<Vec<g::Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub tool_config: Option<g::ToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub safety_settings: Option<Vec<g::SafetySetting>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub cached_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub continuation_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub service_tier: Option<g::ServiceTier>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub(crate) fn present_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
