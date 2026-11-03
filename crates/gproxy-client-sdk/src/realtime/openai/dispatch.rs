use super::*;
/// The protocol enum is untagged: dispatch before deserializing its concrete DTO.
pub(super) fn dispatch(value: Value) -> Result<Option<o::RealtimeServerEvent>> {
    let Some(tag) = value.get("type").and_then(Value::as_str) else {
        return Ok(None);
    };
    macro_rules! decode {
        ($ty:ty, $variant:ident) => {
            Some(o::RealtimeServerEvent::$variant(
                serde_json::from_value::<$ty>(value).map_err(Error::invalid)?,
            ))
        };
    }
    Ok(match tag {
        "conversation.created" => decode!(o::ConversationCreatedEvent, ConversationCreatedEvent),
        "conversation.item.created" => decode!(
            o::ConversationItemCreatedEvent,
            ConversationItemCreatedEvent
        ),
        "conversation.item.deleted" => decode!(
            o::ConversationItemDeletedEvent,
            ConversationItemDeletedEvent
        ),
        "conversation.item.input_audio_transcription.completed" => decode!(
            o::ConversationItemInputAudioTranscriptionCompletedEvent,
            ConversationItemInputAudioTranscriptionCompletedEvent
        ),
        "conversation.item.input_audio_transcription.delta" => decode!(
            o::ConversationItemInputAudioTranscriptionDeltaEvent,
            ConversationItemInputAudioTranscriptionDeltaEvent
        ),
        "conversation.item.input_audio_transcription.failed" => decode!(
            o::ConversationItemInputAudioTranscriptionFailedEvent,
            ConversationItemInputAudioTranscriptionFailedEvent
        ),
        "conversation.item.retrieved" => {
            decode!(o::ConversationItemRetrieved, ConversationItemRetrieved)
        }
        "conversation.item.truncated" => decode!(
            o::ConversationItemTruncatedEvent,
            ConversationItemTruncatedEvent
        ),
        "error" => decode!(o::RealtimeErrorEvent, RealtimeErrorEvent),
        "input_audio_buffer.cleared" => decode!(
            o::InputAudioBufferClearedEvent,
            InputAudioBufferClearedEvent
        ),
        "input_audio_buffer.committed" => decode!(
            o::InputAudioBufferCommittedEvent,
            InputAudioBufferCommittedEvent
        ),
        "input_audio_buffer.dtmf_event_received" => decode!(
            o::InputAudioBufferDtmfEventReceivedEvent,
            InputAudioBufferDtmfEventReceivedEvent
        ),
        "input_audio_buffer.speech_started" => decode!(
            o::InputAudioBufferSpeechStartedEvent,
            InputAudioBufferSpeechStartedEvent
        ),
        "input_audio_buffer.speech_stopped" => decode!(
            o::InputAudioBufferSpeechStoppedEvent,
            InputAudioBufferSpeechStoppedEvent
        ),
        "rate_limits.updated" => decode!(o::RateLimitsUpdatedEvent, RateLimitsUpdatedEvent),
        "response.output_audio.delta" => {
            decode!(o::ResponseAudioDeltaEvent, ResponseAudioDeltaEvent)
        }
        "response.output_audio.done" => decode!(o::ResponseAudioDoneEvent, ResponseAudioDoneEvent),
        "response.output_audio_transcript.delta" => decode!(
            o::ResponseAudioTranscriptDeltaEvent,
            ResponseAudioTranscriptDeltaEvent
        ),
        "response.output_audio_transcript.done" => decode!(
            o::ResponseAudioTranscriptDoneEvent,
            ResponseAudioTranscriptDoneEvent
        ),
        "response.content_part.added" => decode!(
            o::ResponseContentPartAddedEvent,
            ResponseContentPartAddedEvent
        ),
        "response.content_part.done" => decode!(
            o::ResponseContentPartDoneEvent,
            ResponseContentPartDoneEvent
        ),
        "response.created" => decode!(o::ResponseCreatedEvent, ResponseCreatedEvent),
        "response.done" => decode!(o::ResponseDoneEvent, ResponseDoneEvent),
        "response.function_call_arguments.delta" => decode!(
            o::ResponseFunctionCallArgumentsDeltaEvent,
            ResponseFunctionCallArgumentsDeltaEvent
        ),
        "response.function_call_arguments.done" => decode!(
            o::ResponseFunctionCallArgumentsDoneEvent,
            ResponseFunctionCallArgumentsDoneEvent
        ),
        "response.output_item.added" => decode!(
            o::ResponseOutputItemAddedEvent,
            ResponseOutputItemAddedEvent
        ),
        "response.output_item.done" => {
            decode!(o::ResponseOutputItemDoneEvent, ResponseOutputItemDoneEvent)
        }
        "response.output_text.delta" => decode!(o::ResponseTextDeltaEvent, ResponseTextDeltaEvent),
        "response.output_text.done" => decode!(o::ResponseTextDoneEvent, ResponseTextDoneEvent),
        "session.created" => decode!(o::SessionCreatedEvent, SessionCreatedEvent),
        "session.updated" => decode!(o::SessionUpdatedEvent, SessionUpdatedEvent),
        "output_audio_buffer.started" => decode!(
            o::RealtimeServerEventOutputAudioBufferStarted,
            RealtimeServerEventOutputAudioBufferStarted
        ),
        "output_audio_buffer.stopped" => decode!(
            o::RealtimeServerEventOutputAudioBufferStopped,
            RealtimeServerEventOutputAudioBufferStopped
        ),
        "output_audio_buffer.cleared" => decode!(
            o::RealtimeServerEventOutputAudioBufferCleared,
            RealtimeServerEventOutputAudioBufferCleared
        ),
        "conversation.item.added" => decode!(o::ConversationItemAdded, ConversationItemAdded),
        "conversation.item.done" => decode!(o::ConversationItemDone, ConversationItemDone),
        "input_audio_buffer.timeout_triggered" => decode!(
            o::InputAudioBufferTimeoutTriggered,
            InputAudioBufferTimeoutTriggered
        ),
        "conversation.item.input_audio_transcription.segment" => decode!(
            o::ConversationItemInputAudioTranscriptionSegment,
            ConversationItemInputAudioTranscriptionSegment
        ),
        "mcp_list_tools.in_progress" => decode!(o::McpListToolsInProgress, McpListToolsInProgress),
        "mcp_list_tools.completed" => decode!(o::McpListToolsCompleted, McpListToolsCompleted),
        "mcp_list_tools.failed" => decode!(o::McpListToolsFailed, McpListToolsFailed),
        "response.mcp_call_arguments.delta" => decode!(
            o::ResponseMcpCallArgumentsDelta,
            ResponseMcpCallArgumentsDelta
        ),
        "response.mcp_call_arguments.done" => decode!(
            o::ResponseMcpCallArgumentsDone,
            ResponseMcpCallArgumentsDone
        ),
        "response.mcp_call.in_progress" => {
            decode!(o::ResponseMcpCallInProgress, ResponseMcpCallInProgress)
        }
        "response.mcp_call.completed" => {
            decode!(o::ResponseMcpCallCompleted, ResponseMcpCallCompleted)
        }
        "response.mcp_call.failed" => decode!(o::ResponseMcpCallFailed, ResponseMcpCallFailed),
        _ => None,
    })
}
