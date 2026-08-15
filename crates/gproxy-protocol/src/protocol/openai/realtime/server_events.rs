use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;
use super::{
    RealtimeContentPart, RealtimeError, RealtimeItem, RealtimeRateLimit, RealtimeResponse,
    RealtimeSessionConfig, RealtimeUsage,
};

/// Events the server sends over the realtime WebSocket. Unknown event types
/// round-trip through [`UnknownRealtimeServerEvent`]; a known `type` whose
/// payload fails to parse also degrades to `Unknown` so a live session never
/// hard-fails on wire evolution.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RealtimeServerEvent {
    Known(Box<KnownRealtimeServerEvent>),
    Unknown(UnknownRealtimeServerEvent),
}

impl<'de> Deserialize<'de> for RealtimeServerEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        if let Ok(event) = serde_json::from_value(value.clone()) {
            return Ok(Self::Known(Box::new(event)));
        }
        serde_json::from_value(value)
            .map(Self::Unknown)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct UnknownRealtimeServerEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
#[non_exhaustive]
pub enum KnownRealtimeServerEvent {
    #[serde(rename = "error")]
    Error {
        error: RealtimeError,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "session.created")]
    SessionCreated {
        session: Box<RealtimeSessionConfig>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "session.updated")]
    SessionUpdated {
        session: Box<RealtimeSessionConfig>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.added")]
    ConversationItemAdded {
        item: RealtimeItem,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    /// Beta-era alias of `conversation.item.added` still emitted by some peers.
    #[serde(rename = "conversation.item.created")]
    ConversationItemCreated {
        item: RealtimeItem,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.done")]
    ConversationItemDone {
        item: RealtimeItem,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.retrieved")]
    ConversationItemRetrieved {
        item: RealtimeItem,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.truncated")]
    ConversationItemTruncated {
        item_id: String,
        content_index: u32,
        audio_end_ms: u64,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.deleted")]
    ConversationItemDeleted {
        item_id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.input_audio_transcription.delta")]
    InputAudioTranscriptionDelta {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_index: Option<u32>,
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    InputAudioTranscriptionCompleted {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_index: Option<u32>,
        transcript: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<RealtimeUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    InputAudioTranscriptionFailed {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_index: Option<u32>,
        error: RealtimeError,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "conversation.item.input_audio_transcription.segment")]
    InputAudioTranscriptionSegment {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        speaker: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        start: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        end: Option<f64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioBufferCommitted {
        item_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        previous_item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio_buffer.cleared")]
    InputAudioBufferCleared {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio_buffer.speech_started")]
    InputAudioBufferSpeechStarted {
        item_id: String,
        audio_start_ms: u64,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    InputAudioBufferSpeechStopped {
        item_id: String,
        audio_end_ms: u64,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "input_audio_buffer.timeout_triggered")]
    InputAudioBufferTimeoutTriggered {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        audio_start_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        audio_end_ms: Option<u64>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "output_audio_buffer.started")]
    OutputAudioBufferStarted {
        response_id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "output_audio_buffer.stopped")]
    OutputAudioBufferStopped {
        response_id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "output_audio_buffer.cleared")]
    OutputAudioBufferCleared {
        response_id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated {
        rate_limits: Vec<RealtimeRateLimit>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: Box<RealtimeResponse>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.done")]
    ResponseDone {
        response: Box<RealtimeResponse>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded {
        response_id: String,
        output_index: u32,
        item: RealtimeItem,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone {
        response_id: String,
        output_index: u32,
        item: RealtimeItem,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: RealtimeContentPart,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        part: RealtimeContentPart,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_audio_transcript.delta")]
    OutputAudioTranscriptDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_audio_transcript.done")]
    OutputAudioTranscriptDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        transcript: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_audio.delta")]
    OutputAudioDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        /// Base64-encoded audio in the session output format.
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.output_audio.done")]
    OutputAudioDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        content_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        call_id: String,
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        call_id: String,
        arguments: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpCallArgumentsDelta {
        response_id: String,
        item_id: String,
        output_index: u32,
        delta: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpCallArgumentsDone {
        response_id: String,
        item_id: String,
        output_index: u32,
        arguments: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.in_progress")]
    McpCallInProgress {
        item_id: String,
        output_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.completed")]
    McpCallCompleted {
        item_id: String,
        output_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "response.mcp_call.failed")]
    McpCallFailed {
        item_id: String,
        output_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_list_tools.in_progress")]
    McpListToolsInProgress {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_list_tools.completed")]
    McpListToolsCompleted {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "mcp_list_tools.failed")]
    McpListToolsFailed {
        #[serde(skip_serializing_if = "Option::is_none")]
        item_id: Option<String>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}
