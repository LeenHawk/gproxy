use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{JsonObject, ThinkingDisplay, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThinkingConfig {
    Enabled(ThinkingEnabled),
    Disabled(ThinkingDisabled),
    Adaptive(ThinkingAdaptive),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingEnabled {
    pub budget_tokens: u64,
    #[serde(rename = "type")]
    pub type_: ThinkingEnabledType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<ThinkingDisplay>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinkingEnabledType {
    #[serde(rename = "enabled")]
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThinkingDisabled {
    #[serde(rename = "type")]
    pub type_: ThinkingDisabledType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThinkingAdaptiveType {
    #[serde(rename = "adaptive")]
    Adaptive,
}
