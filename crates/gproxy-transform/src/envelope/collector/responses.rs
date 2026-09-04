use gproxy_protocol::openai;

use super::SseFrame;
use crate::TransformError;

#[derive(Default)]
pub(super) struct ResponsesCollector {
    pub(super) response: Option<Box<openai::ResponseObject>>,
}

impl ResponsesCollector {
    pub(super) fn frame(&mut self, frame: SseFrame) -> Result<(), TransformError> {
        let event: openai::ResponseStreamEvent = serde_json::from_str(&frame.data)?;
        match event {
            openai::ResponseStreamEvent::Known(event) => match *event {
                openai::KnownResponseStreamEvent::ResponseCompleted(event)
                | openai::KnownResponseStreamEvent::ResponseIncomplete(event)
                | openai::KnownResponseStreamEvent::ResponseFailed(event) => {
                    self.response = Some(event.response);
                }
                openai::KnownResponseStreamEvent::ResponseCreated(_)
                | openai::KnownResponseStreamEvent::ResponseInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseQueued(_)
                | openai::KnownResponseStreamEvent::ResponseInjectCreated(_)
                | openai::KnownResponseStreamEvent::ResponseInjectFailed(_)
                | openai::KnownResponseStreamEvent::ResponseSteerAccepted(_)
                | openai::KnownResponseStreamEvent::ResponseSteerPending(_)
                | openai::KnownResponseStreamEvent::ResponseSteerFailed(_)
                | openai::KnownResponseStreamEvent::ResponseOutputItemAdded(_)
                | openai::KnownResponseStreamEvent::ResponseOutputItemDone(_)
                | openai::KnownResponseStreamEvent::ResponseContentPartAdded(_)
                | openai::KnownResponseStreamEvent::ResponseContentPartDone(_)
                | openai::KnownResponseStreamEvent::ResponseOutputTextDelta(_)
                | openai::KnownResponseStreamEvent::ResponseOutputTextDone(_)
                | openai::KnownResponseStreamEvent::ResponseOutputTextAnnotationAdded(_)
                | openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(_)
                | openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(_)
                | openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta(_)
                | openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(_)
                | openai::KnownResponseStreamEvent::ResponseRefusalDelta(_)
                | openai::KnownResponseStreamEvent::ResponseRefusalDone(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartAdded(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartDone(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDone(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(_)
                | openai::KnownResponseStreamEvent::ResponseReasoningTextDone(_)
                | openai::KnownResponseStreamEvent::ResponseAudioDelta(_)
                | openai::KnownResponseStreamEvent::ResponseAudioDone(_)
                | openai::KnownResponseStreamEvent::ResponseAudioTranscriptDelta(_)
                | openai::KnownResponseStreamEvent::ResponseAudioTranscriptDone(_)
                | openai::KnownResponseStreamEvent::ResponseImageGenerationCallCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseImageGenerationCallGenerating(_)
                | openai::KnownResponseStreamEvent::ResponseImageGenerationCallInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseImageGenerationCallPartialImage(_)
                | openai::KnownResponseStreamEvent::ResponseFileSearchCallInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseFileSearchCallSearching(_)
                | openai::KnownResponseStreamEvent::ResponseFileSearchCallCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseWebSearchCallInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseWebSearchCallSearching(_)
                | openai::KnownResponseStreamEvent::ResponseWebSearchCallCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallInterpreting(_)
                | openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDelta(_)
                | openai::KnownResponseStreamEvent::ResponseCodeInterpreterCallCodeDone(_)
                | openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDelta(_)
                | openai::KnownResponseStreamEvent::ResponseMcpCallArgumentsDone(_)
                | openai::KnownResponseStreamEvent::ResponseMcpCallInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseMcpCallCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseMcpCallFailed(_)
                | openai::KnownResponseStreamEvent::ResponseMcpListToolsInProgress(_)
                | openai::KnownResponseStreamEvent::ResponseMcpListToolsCompleted(_)
                | openai::KnownResponseStreamEvent::ResponseMcpListToolsFailed(_)
                | openai::KnownResponseStreamEvent::Error(_) => {}
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            },
            openai::ResponseStreamEvent::Unknown(_) => {}
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<openai::ResponseObject, TransformError> {
        self.response
            .map(|response| *response)
            .ok_or(TransformError::IncompleteStream)
    }
}
