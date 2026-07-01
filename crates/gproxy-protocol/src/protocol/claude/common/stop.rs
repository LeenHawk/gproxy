use serde::{Deserialize, Serialize};

use super::{ClaudeModel, JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StopDetails {
    Refusal(RefusalStopDetails),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefusalStopDetails {
    pub category: Option<RefusalCategory>,
    pub explanation: Option<String>,
    #[serde(rename = "type")]
    pub type_: RefusalStopDetailsType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_credit_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_has_prefill_claim: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_model: Option<ClaudeModel>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefusalStopDetailsType {
    #[serde(rename = "refusal")]
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RefusalCategory {
    Known(RefusalCategoryKnown),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefusalCategoryKnown {
    #[serde(rename = "cyber")]
    Cyber,
    #[serde(rename = "bio")]
    Bio,
    #[serde(rename = "reasoning_extraction")]
    ReasoningExtraction,
}
