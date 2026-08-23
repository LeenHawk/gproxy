use serde::{Deserialize, Serialize};

use super::{HarmBlockThreshold, HarmCategory, HarmProbability};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<HarmCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<HarmBlockThreshold>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SafetyRating {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<HarmCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probability: Option<HarmProbability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
