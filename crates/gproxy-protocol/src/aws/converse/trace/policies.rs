use serde::{Deserialize, Serialize};

use crate::aws::{GuardrailOrigin, GuardrailOwnership, Rest};

use super::{
    GuardrailContentFilter, GuardrailContextualGroundingFilter, GuardrailCustomWord,
    GuardrailManagedWord, GuardrailPiiEntityFilter, GuardrailRegexFilter, GuardrailTopic,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct AppliedGuardrailDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_arn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_origin: Option<Vec<GuardrailOrigin>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_ownership: Option<GuardrailOwnership>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_version: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailContentPolicyAssessment {
    pub filters: Vec<GuardrailContentFilter>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailContextualGroundingPolicyAssessment {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<GuardrailContextualGroundingFilter>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailInvocationMetrics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_coverage: Option<GuardrailCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guardrail_processing_latency: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<GuardrailUsage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailCoverage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub images: Option<GuardrailImageCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_characters: Option<GuardrailTextCharactersCoverage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailImageCoverage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guarded: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i32>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type GuardrailTextCharactersCoverage = GuardrailImageCoverage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailUsage {
    pub content_policy_units: i32,
    pub contextual_grounding_policy_units: i32,
    pub sensitive_information_policy_free_units: i32,
    pub sensitive_information_policy_units: i32,
    pub topic_policy_units: i32,
    pub word_policy_units: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automated_reasoning_policies: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automated_reasoning_policy_units: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_policy_image_units: Option<i32>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailSensitiveInformationPolicyAssessment {
    pub pii_entities: Vec<GuardrailPiiEntityFilter>,
    pub regexes: Vec<GuardrailRegexFilter>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailTopicPolicyAssessment {
    pub topics: Vec<GuardrailTopic>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GuardrailWordPolicyAssessment {
    pub custom_words: Vec<GuardrailCustomWord>,
    pub managed_word_lists: Vec<GuardrailManagedWord>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
