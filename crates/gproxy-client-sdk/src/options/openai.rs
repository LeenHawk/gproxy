use super::present_nullable;
use gproxy_protocol::{
    Rest,
    openai::{
        chat::{request as q, tools as qt},
        responses::{generate as o, input as i, tools as ot},
    },
};
use serde::{Deserialize, Serialize};

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
