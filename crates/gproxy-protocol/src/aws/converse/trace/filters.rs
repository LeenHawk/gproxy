use serde::{Deserialize, Serialize};

use crate::aws::{
    GuardrailBlockAction, GuardrailContentFilterType, GuardrailContextualGroundingFilterType,
    GuardrailLevel, GuardrailManagedWordType, GuardrailPiiEntityType, GuardrailSensitiveAction,
    GuardrailTopicType, Rest,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailContentFilter {
    pub action: GuardrailBlockAction,
    pub confidence: GuardrailLevel,
    #[serde(rename = "type")]
    pub type_: GuardrailContentFilterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_strength: Option<GuardrailLevel>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailContextualGroundingFilter {
    pub action: GuardrailBlockAction,
    pub score: f64,
    pub threshold: f64,
    #[serde(rename = "type")]
    pub type_: GuardrailContextualGroundingFilterType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailPiiEntityFilter {
    pub action: GuardrailSensitiveAction,
    #[serde(rename = "match")]
    pub match_: String,
    #[serde(rename = "type")]
    pub type_: GuardrailPiiEntityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailRegexFilter {
    pub action: GuardrailSensitiveAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(rename = "match", skip_serializing_if = "Option::is_none")]
    pub match_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailTopic {
    pub action: GuardrailBlockAction,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: GuardrailTopicType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailCustomWord {
    pub action: GuardrailBlockAction,
    #[serde(rename = "match")]
    pub match_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuardrailManagedWord {
    pub action: GuardrailBlockAction,
    #[serde(rename = "match")]
    pub match_: String,
    #[serde(rename = "type")]
    pub type_: GuardrailManagedWordType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
