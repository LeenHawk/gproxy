use serde::{Deserialize, Serialize};

use super::ThinkingDisplay;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ThinkingConfig {
    Enabled(ThinkingEnabled),
    Disabled(ThinkingDisabled),
    Adaptive(ThinkingAdaptive),
    Unknown(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingEnabled {
    pub budget_tokens: u64,
    #[serde(rename = "type")]
    pub type_: ThinkingEnabledType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ThinkingEnabledType {
    #[serde(rename = "enabled")]
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingDisabled {
    #[serde(rename = "type")]
    pub type_: ThinkingDisabledType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ThinkingDisabledType {
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingAdaptive {
    #[serde(rename = "type")]
    pub type_: ThinkingAdaptiveType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ThinkingAdaptiveType {
    #[serde(rename = "adaptive")]
    Adaptive,
}
