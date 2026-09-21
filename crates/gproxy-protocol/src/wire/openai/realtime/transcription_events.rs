//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use super::{LogProbProperties, TranscriptionLanguage};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_completed_event.py`, `ConversationItemInputAudioTranscriptionCompletedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionCompletedEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub transcript: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemInputAudioTranscriptionCompletedEventType,
    pub usage: ConversationItemInputAudioTranscriptionCompletedEventUsage,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub languages: Option<Option<Vec<TranscriptionLanguage>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub logprobs: Option<Option<Vec<LogProbProperties>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_delta_event.py`, `ConversationItemInputAudioTranscriptionDeltaEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionDeltaEvent {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemInputAudioTranscriptionDeltaEventType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub content_index: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub delta: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub logprobs: Option<Option<Vec<LogProbProperties>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_failed_event.py`, `ConversationItemInputAudioTranscriptionFailedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionFailedEvent {
    pub content_index: i64,
    pub error: ConversationItemInputAudioTranscriptionFailedEventError,
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemInputAudioTranscriptionFailedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_segment.py`, `ConversationItemInputAudioTranscriptionSegment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionSegment {
    pub id: String,
    pub content_index: i64,
    pub end: serde_json::Number,
    pub event_id: String,
    pub item_id: String,
    pub speaker: String,
    pub start: serde_json::Number,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemInputAudioTranscriptionSegmentType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_completed_event.py`, `Usage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionCompletedEventUsage {
    ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokens(
        ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokens,
    ),
    ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDuration(
        ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDuration,
    ),
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_failed_event.py`, `Error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionFailedEventError {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub code: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub message: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub param: Option<Option<String>>,
    #[serde(rename = "type")]
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_completed_event.py`, `UsageTranscriptTextUsageTokens`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokens {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    #[serde(rename = "type")]
    pub type_: ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensType,
    #[serde(default, deserialize_with = "present_nullable", skip_serializing_if = "Option::is_none")]
    pub input_token_details: Option<Option<ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensInputTokenDetails>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_completed_event.py`, `UsageTranscriptTextUsageDuration`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDuration {
    pub seconds: serde_json::Number,
    #[serde(rename = "type")]
    pub type_:
        ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDurationType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_input_audio_transcription_completed_event.py`, `UsageTranscriptTextUsageTokensInputTokenDetails`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensInputTokenDetails
{
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub text_tokens: Option<Option<i64>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionCompletedEventType {
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    ConversationItemInputAudioTranscriptionCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionDeltaEventType {
    #[serde(rename = "conversation.item.input_audio_transcription.delta")]
    ConversationItemInputAudioTranscriptionDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionFailedEventType {
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    ConversationItemInputAudioTranscriptionFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionSegmentType {
    #[serde(rename = "conversation.item.input_audio_transcription.segment")]
    ConversationItemInputAudioTranscriptionSegment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageTokensType {
    #[serde(rename = "tokens")]
    Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ConversationItemInputAudioTranscriptionCompletedEventUsageTranscriptTextUsageDurationType {
    #[serde(rename = "duration")]
    Duration,
}
