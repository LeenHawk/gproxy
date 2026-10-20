//! Custom voice creation, OpenAI reference verified 2026-10-20.
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CreatePromptVoiceRequestBody {
    #[serde(rename = "type")]
    pub type_: PromptVoiceType,
    pub name: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_hint: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PromptVoiceType {
    Prompt,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum VoiceSource {
    Prompt,
    AudioSample,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct Voice {
    pub id: String,
    pub created_at: u64,
    pub name: String,
    pub object: String,
    #[serde(rename = "type")]
    pub type_: VoiceSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, gproxy_protocol_macros::WireBuilder, gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateSampleVoiceMultipartForm {
    pub name: String,
    pub consent: String,
    pub audio_sample: crate::connection::MultipartPart,
    #[cfg_attr(feature = "schema", schemars(rename = "type"))]
    pub type_: Option<VoiceSource>,
}
pub type CreatePromptVoiceRequest = WireRequest<CreatePromptVoiceRequestBody>;
pub type CreateSampleVoiceRequest = WireRequest<CreateSampleVoiceMultipartForm>;
pub type CreateVoiceResponse = WireResponse<Voice>;
