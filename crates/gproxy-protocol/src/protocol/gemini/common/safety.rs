use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ExtraFields, HarmBlockThreshold, HarmCategory, HarmProbability};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SafetySetting {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<HarmCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<HarmBlockThreshold>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}
