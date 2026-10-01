//! Standard OpenAI `/v1/moderations`, not Codex Guardian. Source: openai-python
//! src/openai/types/{moderation_create_params,moderation_create_response,
//! moderation,moderation_text_input_param,moderation_image_url_input_param}.py
//! (verified 2026-10-01). Category keys remain open as upstream adds categories.
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CreateModerationRequestBody {
    pub input: ModerationInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ModerationInput {
    Text(String),
    Texts(Vec<String>),
    Multimodal(Vec<ModerationContent>),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ModerationContent {
    Text {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    ImageUrl {
        image_url: ModerationImageUrl,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationImageUrl {
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CreateModerationResponseBody {
    pub id: String,
    pub model: String,
    pub results: Vec<ModerationResult>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ModerationResult {
    pub flagged: bool,
    pub categories: BTreeMap<String, Option<bool>>,
    pub category_scores: BTreeMap<String, serde_json::Number>,
    pub category_applied_input_types: BTreeMap<String, Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type CreateModerationRequest = WireRequest<CreateModerationRequestBody>;
pub type CreateModerationResponse = WireResponse<CreateModerationResponseBody>;
