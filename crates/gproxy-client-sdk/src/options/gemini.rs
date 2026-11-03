use gproxy_protocol::{Rest, gemini as g};
use serde::{Deserialize, Serialize};

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
