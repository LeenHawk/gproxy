use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::{ClaudeModel, JsonObject, RefusalCategory};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackBlockParam {
    pub from: FallbackInfo,
    pub to: FallbackInfo,
    #[serde(rename = "type")]
    pub type_: FallbackBlockType,
    // Claude accepts any JSON value or null and requires a prior trigger echoed verbatim.
    #[serde(
        default,
        deserialize_with = "deserialize_present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger: Option<Value>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFallbackBlock {
    pub from: FallbackInfo,
    pub to: FallbackInfo,
    pub trigger: FallbackRefusalTrigger,
    #[serde(rename = "type")]
    pub type_: FallbackBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackRefusalTrigger {
    pub category: RefusalCategory,
    #[serde(rename = "type")]
    pub type_: FallbackRefusalTriggerType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackInfo {
    pub model: ClaudeModel,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FallbackBlockType {
    #[serde(rename = "fallback")]
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum FallbackRefusalTriggerType {
    #[serde(rename = "refusal")]
    Refusal,
}

fn deserialize_present_value<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
}
