use serde::{Deserialize, Serialize};

use super::{ClaudeModel, JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum StopDetails {
    Refusal(RefusalStopDetails),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum RefusalStopDetailsType {
    #[serde(rename = "refusal")]
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RefusalCategory {
    Known(RefusalCategoryKnown),
    Unknown(String),
}

// Manual Deserialize: unknown values fall back without formatting an
// unknown-variant error (see `protocol::extensible`).
impl<'de> serde::Deserialize<'de> for RefusalCategory {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        crate::protocol::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum RefusalCategoryKnown {
    #[serde(rename = "cyber")]
    Cyber,
    #[serde(rename = "bio")]
    Bio,
    #[serde(rename = "reasoning_extraction")]
    ReasoningExtraction,
    #[serde(rename = "frontier_llm")]
    FrontierLlm,
    #[serde(rename = "general_harms")]
    GeneralHarms,
}
