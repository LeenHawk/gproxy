//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use super::{
    RealtimeAudioConfig, RealtimeReasoning, RealtimeToolChoiceConfig, RealtimeToolsConfig,
    RealtimeTracingConfig, RealtimeTranscriptionSessionCreateRequest, RealtimeTruncation,
    ResponsePrompt,
};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/realtime_session_create_request.py`, `RealtimeSessionCreateRequest`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateRequest {
    #[serde(rename = "type")]
    pub type_: RealtimeSessionCreateRequestType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio: Option<Option<RealtimeAudioConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub include: Option<Option<Vec<RealtimeSessionCreateRequestIncludeItem>>>,
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
    pub max_output_tokens: Option<Option<RealtimeSessionCreateRequestMaxOutputTokens>>,
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
    pub output_modalities: Option<Option<Vec<RealtimeSessionCreateRequestOutputModalitiesItem>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub parallel_tool_calls: Option<Option<bool>>,
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
    pub tool_choice: Option<Option<RealtimeToolChoiceConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tools: Option<Option<RealtimeToolsConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tracing: Option<Option<RealtimeTracingConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub truncation: Option<Option<RealtimeTruncation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/session_created_event.py`, `SessionCreatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SessionCreatedEvent {
    pub event_id: String,
    pub session: SessionCreatedEventSession,
    #[serde(rename = "type")]
    pub type_: SessionCreatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/session_updated_event.py`, `SessionUpdatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SessionUpdatedEvent {
    pub event_id: String,
    pub session: SessionUpdatedEventSession,
    #[serde(rename = "type")]
    pub type_: SessionUpdatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/session_created_event.py`, `Session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum SessionCreatedEventSession {
    RealtimeSessionCreateRequest(RealtimeSessionCreateRequest),
    RealtimeTranscriptionSessionCreateRequest(RealtimeTranscriptionSessionCreateRequest),
}

/// Source: `openai/types/realtime/session_updated_event.py`, `Session`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum SessionUpdatedEventSession {
    RealtimeSessionCreateRequest(RealtimeSessionCreateRequest),
    RealtimeTranscriptionSessionCreateRequest(RealtimeTranscriptionSessionCreateRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateRequestType {
    #[serde(rename = "realtime")]
    Realtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateRequestIncludeItem {
    #[serde(rename = "item.input_audio_transcription.logprobs")]
    ItemInputAudioTranscriptionLogprobs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateRequestMaxOutputTokensVariant2 {
    #[serde(rename = "inf")]
    Inf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateRequestMaxOutputTokens {
    Number(i64),
    Literal(RealtimeSessionCreateRequestMaxOutputTokensVariant2),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateRequestOutputModalitiesItem {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "audio")]
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SessionCreatedEventType {
    #[serde(rename = "session.created")]
    SessionCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SessionUpdatedEventType {
    #[serde(rename = "session.updated")]
    SessionUpdated,
}
