//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/input_audio_buffer_cleared_event.py`, `InputAudioBufferClearedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferClearedEvent {
    pub event_id: String,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferClearedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/input_audio_buffer_committed_event.py`, `InputAudioBufferCommittedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferCommittedEvent {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferCommittedEventType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_item_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/input_audio_buffer_dtmf_event_received_event.py`, `InputAudioBufferDtmfEventReceivedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferDtmfEventReceivedEvent {
    pub event: String,
    pub received_at: i64,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferDtmfEventReceivedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/input_audio_buffer_speech_started_event.py`, `InputAudioBufferSpeechStartedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferSpeechStartedEvent {
    pub audio_start_ms: i64,
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferSpeechStartedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/input_audio_buffer_speech_stopped_event.py`, `InputAudioBufferSpeechStoppedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferSpeechStoppedEvent {
    pub audio_end_ms: i64,
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferSpeechStoppedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_audio_delta_event.py`, `ResponseAudioDeltaEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAudioDeltaEvent {
    pub content_index: i64,
    pub delta: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseAudioDeltaEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_audio_done_event.py`, `ResponseAudioDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAudioDoneEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseAudioDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_audio_transcript_delta_event.py`, `ResponseAudioTranscriptDeltaEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAudioTranscriptDeltaEvent {
    pub content_index: i64,
    pub delta: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseAudioTranscriptDeltaEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_audio_transcript_done_event.py`, `ResponseAudioTranscriptDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ResponseAudioTranscriptDoneEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    pub transcript: String,
    #[serde(rename = "type")]
    pub type_: ResponseAudioTranscriptDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/input_audio_buffer_timeout_triggered.py`, `InputAudioBufferTimeoutTriggered`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InputAudioBufferTimeoutTriggered {
    pub audio_end_ms: i64,
    pub audio_start_ms: i64,
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: InputAudioBufferTimeoutTriggeredType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferClearedEventType {
    #[serde(rename = "input_audio_buffer.cleared")]
    InputAudioBufferCleared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferCommittedEventType {
    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioBufferCommitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferDtmfEventReceivedEventType {
    #[serde(rename = "input_audio_buffer.dtmf_event_received")]
    InputAudioBufferDtmfEventReceived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferSpeechStartedEventType {
    #[serde(rename = "input_audio_buffer.speech_started")]
    InputAudioBufferSpeechStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferSpeechStoppedEventType {
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    InputAudioBufferSpeechStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAudioDeltaEventType {
    #[serde(rename = "response.output_audio.delta")]
    ResponseOutputAudioDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAudioDoneEventType {
    #[serde(rename = "response.output_audio.done")]
    ResponseOutputAudioDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAudioTranscriptDeltaEventType {
    #[serde(rename = "response.output_audio_transcript.delta")]
    ResponseOutputAudioTranscriptDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseAudioTranscriptDoneEventType {
    #[serde(rename = "response.output_audio_transcript.done")]
    ResponseOutputAudioTranscriptDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InputAudioBufferTimeoutTriggeredType {
    #[serde(rename = "input_audio_buffer.timeout_triggered")]
    InputAudioBufferTimeoutTriggered,
}
