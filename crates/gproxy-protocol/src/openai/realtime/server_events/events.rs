use serde::{Deserialize, Serialize};

use super::payloads::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownRealtimeServerEvent {
    #[serde(rename = "error")]
    Error(RealtimeErrorEvent),
    #[serde(rename = "session.created")]
    SessionCreated(RealtimeSessionEvent),
    #[serde(rename = "session.updated")]
    SessionUpdated(RealtimeSessionEvent),
    #[serde(rename = "conversation.item.added")]
    ConversationItemAdded(RealtimeConversationItemEvent),
    #[serde(rename = "conversation.created")]
    ConversationCreated(RealtimeConversationCreatedEvent),
    #[serde(rename = "conversation.item.created")]
    ConversationItemCreated(RealtimeConversationItemEvent),
    #[serde(rename = "conversation.item.done")]
    ConversationItemDone(RealtimeConversationItemEvent),
    #[serde(rename = "conversation.item.retrieved")]
    ConversationItemRetrieved(RealtimeConversationItemRetrievedEvent),
    #[serde(rename = "conversation.item.truncated")]
    ConversationItemTruncated(RealtimeConversationItemTruncatedEvent),
    #[serde(rename = "conversation.item.deleted")]
    ConversationItemDeleted(RealtimeConversationItemDeletedEvent),
    #[serde(rename = "conversation.item.input_audio_transcription.delta")]
    InputAudioTranscriptionDelta(RealtimeInputTranscriptionDeltaEvent),
    #[serde(rename = "conversation.item.input_audio_transcription.completed")]
    InputAudioTranscriptionCompleted(RealtimeInputTranscriptionCompletedEvent),
    #[serde(rename = "conversation.item.input_audio_transcription.failed")]
    InputAudioTranscriptionFailed(RealtimeInputTranscriptionFailedEvent),
    #[serde(rename = "conversation.item.input_audio_transcription.segment")]
    InputAudioTranscriptionSegment(RealtimeInputTranscriptionSegmentEvent),
    #[serde(rename = "input_audio_buffer.committed")]
    InputAudioBufferCommitted(RealtimeInputAudioBufferCommittedEvent),
    #[serde(rename = "input_audio_buffer.cleared")]
    InputAudioBufferCleared(RealtimeInputAudioBufferClearedEvent),
    #[serde(rename = "input_audio_buffer.dtmf_event_received")]
    InputAudioBufferDtmfEventReceived(RealtimeDtmfEvent),
    #[serde(rename = "input_audio_buffer.speech_started")]
    InputAudioBufferSpeechStarted(RealtimeInputAudioSpeechStartedEvent),
    #[serde(rename = "input_audio_buffer.speech_stopped")]
    InputAudioBufferSpeechStopped(RealtimeInputAudioSpeechStoppedEvent),
    #[serde(rename = "input_audio_buffer.timeout_triggered")]
    InputAudioBufferTimeoutTriggered(RealtimeInputAudioTimeoutEvent),
    #[serde(rename = "output_audio_buffer.started")]
    OutputAudioBufferStarted(RealtimeOutputAudioBufferEvent),
    #[serde(rename = "output_audio_buffer.stopped")]
    OutputAudioBufferStopped(RealtimeOutputAudioBufferEvent),
    #[serde(rename = "output_audio_buffer.cleared")]
    OutputAudioBufferCleared(RealtimeOutputAudioBufferEvent),
    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated(RealtimeRateLimitsEvent),
    #[serde(rename = "response.created")]
    ResponseCreated(RealtimeResponseEvent),
    #[serde(rename = "response.done")]
    ResponseDone(RealtimeResponseEvent),
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded(RealtimeResponseOutputItemEvent),
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone(RealtimeResponseOutputItemEvent),
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded(RealtimeResponseContentPartEvent),
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone(RealtimeResponseContentPartEvent),
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta(RealtimeResponseOutputDeltaEvent),
    #[serde(rename = "response.output_text.done")]
    OutputTextDone(RealtimeResponseOutputTextDoneEvent),
    #[serde(rename = "response.output_audio_transcript.delta")]
    OutputAudioTranscriptDelta(RealtimeResponseOutputDeltaEvent),
    #[serde(rename = "response.output_audio_transcript.done")]
    OutputAudioTranscriptDone(RealtimeResponseAudioTranscriptDoneEvent),
    #[serde(rename = "response.output_audio.delta")]
    OutputAudioDelta(RealtimeResponseOutputDeltaEvent),
    #[serde(rename = "response.output_audio.done")]
    OutputAudioDone(RealtimeResponseOutputAudioDoneEvent),
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta(RealtimeFunctionArgumentsDeltaEvent),
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone(RealtimeFunctionArgumentsDoneEvent),
    #[serde(rename = "response.mcp_call_arguments.delta")]
    McpCallArgumentsDelta(RealtimeMcpArgumentsDeltaEvent),
    #[serde(rename = "response.mcp_call_arguments.done")]
    McpCallArgumentsDone(RealtimeMcpArgumentsDoneEvent),
    #[serde(rename = "response.mcp_call.in_progress")]
    McpCallInProgress(RealtimeMcpCallStatusEvent),
    #[serde(rename = "response.mcp_call.completed")]
    McpCallCompleted(RealtimeMcpCallStatusEvent),
    #[serde(rename = "response.mcp_call.failed")]
    McpCallFailed(RealtimeMcpCallStatusEvent),
    #[serde(rename = "mcp_list_tools.in_progress")]
    McpListToolsInProgress(RealtimeMcpListToolsStatusEvent),
    #[serde(rename = "mcp_list_tools.completed")]
    McpListToolsCompleted(RealtimeMcpListToolsStatusEvent),
    #[serde(rename = "mcp_list_tools.failed")]
    McpListToolsFailed(RealtimeMcpListToolsStatusEvent),
}
