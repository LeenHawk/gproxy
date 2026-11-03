use gproxy_protocol::{
    Rest,
    claude::{count_tokens as cq, generate_content as c, tools as ct},
};
use serde::{Deserialize, Serialize};

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
