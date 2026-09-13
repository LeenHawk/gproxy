//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use super::{
    RealtimeReasoning, RealtimeSessionCreateResponseAudio, RealtimeSessionCreateResponseTool,
    RealtimeSessionCreateResponseToolChoice, RealtimeTruncation, ResponsePrompt,
};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `RealtimeSessionCreateResponse`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponse {
    pub id: String,
    pub object: RealtimeSessionCreateResponseObject,
    #[serde(rename = "type")]
    pub type_: RealtimeSessionCreateResponseType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio: Option<Option<RealtimeSessionCreateResponseAudio>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub expires_at: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub include: Option<Option<Vec<RealtimeSessionCreateResponseIncludeItem>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_tokens: Option<Option<RealtimeSessionCreateResponseMaxOutputTokens>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub model: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_modalities: Option<Option<Vec<RealtimeSessionCreateResponseOutputModalitiesItem>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt: Option<Option<ResponsePrompt>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<Option<RealtimeReasoning>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_choice: Option<Option<RealtimeSessionCreateResponseToolChoice>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tools: Option<Option<Vec<RealtimeSessionCreateResponseTool>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tracing: Option<Option<RealtimeSessionCreateResponseTracing>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub truncation: Option<Option<RealtimeTruncation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `Tracing`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateResponseTracing {
    Literal(RealtimeSessionCreateResponseTracingVariant1),
    RealtimeSessionCreateResponseTracingTracingConfiguration(
        RealtimeSessionCreateResponseTracingTracingConfiguration,
    ),
    Variant3(()),
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `TracingTracingConfiguration`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseTracingTracingConfiguration {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub group_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub metadata: Option<Option<serde_json::Value>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub workflow_name: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseObject {
    #[serde(rename = "realtime.session")]
    RealtimeSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseType {
    #[serde(rename = "realtime")]
    Realtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseIncludeItem {
    #[serde(rename = "item.input_audio_transcription.logprobs")]
    ItemInputAudioTranscriptionLogprobs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseMaxOutputTokensVariant2 {
    #[serde(rename = "inf")]
    Inf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateResponseMaxOutputTokens {
    Number(i64),
    Literal(RealtimeSessionCreateResponseMaxOutputTokensVariant2),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseOutputModalitiesItem {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "audio")]
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseTracingVariant1 {
    #[serde(rename = "auto")]
    Auto,
}
