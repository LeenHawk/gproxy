//! OpenAI Decisions API (public beta), verified against openai-python on 2026-10-08.
//! `decision_create_params.py`, `decision.py`, and `decision_input_*_param.py`.
//! Choice values distinguish booleans from strings; refusals are per question.
use super::responses::ResponseUsage;
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CreateDecisionRequestBody {
    pub model: String,
    pub input: DecisionInput,
    pub questions: Vec<DecisionQuestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum DecisionInput {
    Text(String),
    Messages(Vec<DecisionInputMessage>),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct DecisionInputMessage {
    pub role: DecisionRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub type_: Option<DecisionMessageType>,
    pub content: DecisionContent,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum DecisionRole {
    User,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMessageType {
    Message,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum DecisionContent {
    Text(String),
    Parts(Vec<DecisionInputPart>),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionInputPart {
    InputText {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    InputImage {
        image_url: String,
        #[serde(
            default,
            deserialize_with = "super::responses::input::present_nullable",
            skip_serializing_if = "Option::is_none"
        )]
        detail: Option<Option<DecisionImageDetail>>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum DecisionImageDetail {
    Low,
    High,
    Auto,
    Original,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum DecisionChoiceValue {
    String(String),
    Boolean(bool),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct DecisionChoice {
    pub value: DecisionChoiceValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct DecisionScoreLevel {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionQuestion {
    Predicate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
        choices: Vec<DecisionChoice>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        instructions: String,
        levels: Vec<DecisionScoreLevel>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CreateDecisionResponseBody {
    pub model: String,
    pub answers: Vec<DecisionAnswer>,
    pub usage: ResponseUsage,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct DecisionChoiceProbability {
    pub value: DecisionChoiceValue,
    pub probability: f64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct DecisionScoreProbability {
    pub label: String,
    pub value: i64,
    pub probability: f64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DecisionAnswer {
    Predicate {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        probability: f64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        choice: DecisionChoiceValue,
        confidence: f64,
        probabilities: Vec<DecisionChoiceProbability>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        score: f64,
        confidence: f64,
        probabilities: Vec<DecisionScoreProbability>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Refusal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
}

pub type CreateDecisionRequest = WireRequest<CreateDecisionRequestBody>;
pub type CreateDecisionResponse = WireResponse<CreateDecisionResponseBody>;
