//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::{
    ConversationCreatedEvent, ConversationItem, ConversationItemAdded, ConversationItemCreateEvent,
    ConversationItemCreatedEvent, ConversationItemDeleteEvent, ConversationItemDeletedEvent,
    ConversationItemDone, ConversationItemInputAudioTranscriptionCompletedEvent,
    ConversationItemInputAudioTranscriptionDeltaEvent,
    ConversationItemInputAudioTranscriptionFailedEvent,
    ConversationItemInputAudioTranscriptionSegment, ConversationItemRetrieveEvent,
    ConversationItemTruncateEvent, ConversationItemTruncatedEvent, InputAudioBufferAppendEvent,
    InputAudioBufferClearEvent, InputAudioBufferClearedEvent, InputAudioBufferCommitEvent,
    InputAudioBufferCommittedEvent, InputAudioBufferDtmfEventReceivedEvent,
    InputAudioBufferSpeechStartedEvent, InputAudioBufferSpeechStoppedEvent,
    InputAudioBufferTimeoutTriggered, McpListToolsCompleted, McpListToolsFailed,
    McpListToolsInProgress, OutputAudioBufferClearEvent, RateLimitsUpdatedEvent,
    RealtimeErrorEvent, ResponseAudioDeltaEvent, ResponseAudioDoneEvent,
    ResponseAudioTranscriptDeltaEvent, ResponseAudioTranscriptDoneEvent, ResponseCancelEvent,
    ResponseContentPartAddedEvent, ResponseContentPartDoneEvent, ResponseCreateEvent,
    ResponseCreatedEvent, ResponseDoneEvent, ResponseFunctionCallArgumentsDeltaEvent,
    ResponseFunctionCallArgumentsDoneEvent, ResponseMcpCallArgumentsDelta,
    ResponseMcpCallArgumentsDone, ResponseMcpCallCompleted, ResponseMcpCallFailed,
    ResponseMcpCallInProgress, ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent,
    ResponseTextDeltaEvent, ResponseTextDoneEvent, SessionCreatedEvent, SessionUpdateEvent,
    SessionUpdatedEvent,
};
use crate::Rest;
use serde::{Deserialize, Serialize};

pub type HandshakeRequest = crate::WireRequest<()>;
pub type HandshakeResponse = crate::WireResponse<()>;

/// Source: `openai/types/realtime/realtime_client_event.py`, `RealtimeClientEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum RealtimeClientEvent {
    ConversationItemCreateEvent(ConversationItemCreateEvent),
    ConversationItemDeleteEvent(ConversationItemDeleteEvent),
    ConversationItemRetrieveEvent(ConversationItemRetrieveEvent),
    ConversationItemTruncateEvent(ConversationItemTruncateEvent),
    InputAudioBufferAppendEvent(InputAudioBufferAppendEvent),
    InputAudioBufferClearEvent(InputAudioBufferClearEvent),
    OutputAudioBufferClearEvent(OutputAudioBufferClearEvent),
    InputAudioBufferCommitEvent(InputAudioBufferCommitEvent),
    ResponseCancelEvent(ResponseCancelEvent),
    ResponseCreateEvent(ResponseCreateEvent),
    SessionUpdateEvent(SessionUpdateEvent),
}

/// Source: `openai/types/realtime/realtime_server_event.py`, `RealtimeServerEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeServerEvent {
    ConversationCreatedEvent(ConversationCreatedEvent),
    ConversationItemCreatedEvent(ConversationItemCreatedEvent),
    ConversationItemDeletedEvent(ConversationItemDeletedEvent),
    ConversationItemInputAudioTranscriptionCompletedEvent(
        ConversationItemInputAudioTranscriptionCompletedEvent,
    ),
    ConversationItemInputAudioTranscriptionDeltaEvent(
        ConversationItemInputAudioTranscriptionDeltaEvent,
    ),
    ConversationItemInputAudioTranscriptionFailedEvent(
        ConversationItemInputAudioTranscriptionFailedEvent,
    ),
    ConversationItemRetrieved(ConversationItemRetrieved),
    ConversationItemTruncatedEvent(ConversationItemTruncatedEvent),
    RealtimeErrorEvent(RealtimeErrorEvent),
    InputAudioBufferClearedEvent(InputAudioBufferClearedEvent),
    InputAudioBufferCommittedEvent(InputAudioBufferCommittedEvent),
    InputAudioBufferDtmfEventReceivedEvent(InputAudioBufferDtmfEventReceivedEvent),
    InputAudioBufferSpeechStartedEvent(InputAudioBufferSpeechStartedEvent),
    InputAudioBufferSpeechStoppedEvent(InputAudioBufferSpeechStoppedEvent),
    RateLimitsUpdatedEvent(RateLimitsUpdatedEvent),
    ResponseAudioDeltaEvent(ResponseAudioDeltaEvent),
    ResponseAudioDoneEvent(ResponseAudioDoneEvent),
    ResponseAudioTranscriptDeltaEvent(ResponseAudioTranscriptDeltaEvent),
    ResponseAudioTranscriptDoneEvent(ResponseAudioTranscriptDoneEvent),
    ResponseContentPartAddedEvent(ResponseContentPartAddedEvent),
    ResponseContentPartDoneEvent(ResponseContentPartDoneEvent),
    ResponseCreatedEvent(ResponseCreatedEvent),
    ResponseDoneEvent(ResponseDoneEvent),
    ResponseFunctionCallArgumentsDeltaEvent(ResponseFunctionCallArgumentsDeltaEvent),
    ResponseFunctionCallArgumentsDoneEvent(ResponseFunctionCallArgumentsDoneEvent),
    ResponseOutputItemAddedEvent(ResponseOutputItemAddedEvent),
    ResponseOutputItemDoneEvent(ResponseOutputItemDoneEvent),
    ResponseTextDeltaEvent(ResponseTextDeltaEvent),
    ResponseTextDoneEvent(ResponseTextDoneEvent),
    SessionCreatedEvent(SessionCreatedEvent),
    SessionUpdatedEvent(SessionUpdatedEvent),
    RealtimeServerEventOutputAudioBufferStarted(RealtimeServerEventOutputAudioBufferStarted),
    RealtimeServerEventOutputAudioBufferStopped(RealtimeServerEventOutputAudioBufferStopped),
    RealtimeServerEventOutputAudioBufferCleared(RealtimeServerEventOutputAudioBufferCleared),
    ConversationItemAdded(ConversationItemAdded),
    ConversationItemDone(ConversationItemDone),
    InputAudioBufferTimeoutTriggered(InputAudioBufferTimeoutTriggered),
    ConversationItemInputAudioTranscriptionSegment(ConversationItemInputAudioTranscriptionSegment),
    McpListToolsInProgress(McpListToolsInProgress),
    McpListToolsCompleted(McpListToolsCompleted),
    McpListToolsFailed(McpListToolsFailed),
    ResponseMcpCallArgumentsDelta(ResponseMcpCallArgumentsDelta),
    ResponseMcpCallArgumentsDone(ResponseMcpCallArgumentsDone),
    ResponseMcpCallInProgress(ResponseMcpCallInProgress),
    ResponseMcpCallCompleted(ResponseMcpCallCompleted),
    ResponseMcpCallFailed(ResponseMcpCallFailed),
}

/// Source: `openai/types/realtime/realtime_server_event.py`, `ConversationItemRetrieved`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemRetrieved {
    pub event_id: String,
    pub item: ConversationItem,
    #[serde(rename = "type")]
    pub type_: ConversationItemRetrievedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_server_event.py`, `OutputAudioBufferStarted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeServerEventOutputAudioBufferStarted {
    pub event_id: String,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: RealtimeServerEventOutputAudioBufferStartedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_server_event.py`, `OutputAudioBufferStopped`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeServerEventOutputAudioBufferStopped {
    pub event_id: String,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: RealtimeServerEventOutputAudioBufferStoppedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_server_event.py`, `OutputAudioBufferCleared`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeServerEventOutputAudioBufferCleared {
    pub event_id: String,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: RealtimeServerEventOutputAudioBufferClearedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemRetrievedType {
    #[serde(rename = "conversation.item.retrieved")]
    ConversationItemRetrieved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeServerEventOutputAudioBufferStartedType {
    #[serde(rename = "output_audio_buffer.started")]
    OutputAudioBufferStarted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeServerEventOutputAudioBufferStoppedType {
    #[serde(rename = "output_audio_buffer.stopped")]
    OutputAudioBufferStopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeServerEventOutputAudioBufferClearedType {
    #[serde(rename = "output_audio_buffer.cleared")]
    OutputAudioBufferCleared,
}
