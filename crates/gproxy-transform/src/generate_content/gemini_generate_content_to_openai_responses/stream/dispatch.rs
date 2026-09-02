use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::State;

impl State {
    pub(super) fn event(
        &mut self,
        event: openai::ResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            return Err(TransformError::shape(
                "Responses stream",
                "event received after terminal event",
            ));
        }
        let event = match event {
            openai::ResponseStreamEvent::Known(event) => event,
            openai::ResponseStreamEvent::Unknown(_) => return Ok(Vec::new()),
        };
        use openai::KnownResponseStreamEvent as E;
        match *event {
            E::ResponseCreated(event) | E::ResponseInProgress(event) | E::ResponseQueued(event) => {
                self.remember(&event.response)?;
                Ok(Vec::new())
            }
            E::ResponseOutputTextDelta(event) => self.text_delta(event.delta, event.item_id, false),
            E::ResponseReasoningTextDelta(event) => {
                self.text_delta(event.delta, event.item_id, true)
            }
            E::ResponseReasoningSummaryTextDelta(event) => {
                self.text_delta(event.delta, event.item_id, true)
            }
            E::ResponseRefusalDelta(event) => self.text_delta(event.delta, event.item_id, false),
            E::ResponseOutputItemAdded(event) => self.item_added(event),
            E::ResponseOutputItemDone(event) => self.item_done(event),
            E::ResponseFunctionCallArgumentsDelta(event) => self.tool_delta(event, false),
            E::ResponseCustomToolCallInputDelta(event) => self.tool_delta(event, true),
            E::ResponseFunctionCallArgumentsDone(event) => self.function_done(event),
            E::ResponseCustomToolCallInputDone(event) => self.custom_done(event),
            E::ResponseCompleted(event) => self.terminal(event, openai::ResponseStatus::Completed),
            E::ResponseIncomplete(event) => {
                self.terminal(event, openai::ResponseStatus::Incomplete)
            }
            E::ResponseFailed(event) => self.terminal(event, openai::ResponseStatus::Failed),
            E::Error(event) => Err(TransformError::unsupported(
                "Responses stream",
                event.message,
            )),
            E::ResponseContentPartAdded(_)
            | E::ResponseInjectCreated(_)
            | E::ResponseInjectFailed(_)
            | E::ResponseContentPartDone(_)
            | E::ResponseOutputTextDone(_)
            | E::ResponseOutputTextAnnotationAdded(_)
            | E::ResponseRefusalDone(_)
            | E::ResponseReasoningSummaryPartAdded(_)
            | E::ResponseReasoningSummaryPartDone(_)
            | E::ResponseReasoningSummaryTextDone(_)
            | E::ResponseReasoningTextDone(_) => Ok(Vec::new()),
            E::ResponseAudioDelta(_)
            | E::ResponseAudioDone(_)
            | E::ResponseAudioTranscriptDelta(_)
            | E::ResponseAudioTranscriptDone(_)
            | E::ResponseImageGenerationCallCompleted(_)
            | E::ResponseImageGenerationCallGenerating(_)
            | E::ResponseImageGenerationCallInProgress(_)
            | E::ResponseImageGenerationCallPartialImage(_)
            | E::ResponseFileSearchCallInProgress(_)
            | E::ResponseFileSearchCallSearching(_)
            | E::ResponseFileSearchCallCompleted(_)
            | E::ResponseWebSearchCallInProgress(_)
            | E::ResponseWebSearchCallSearching(_)
            | E::ResponseWebSearchCallCompleted(_)
            | E::ResponseCodeInterpreterCallInProgress(_)
            | E::ResponseCodeInterpreterCallInterpreting(_)
            | E::ResponseCodeInterpreterCallCompleted(_)
            | E::ResponseCodeInterpreterCallCodeDelta(_)
            | E::ResponseCodeInterpreterCallCodeDone(_)
            | E::ResponseMcpCallArgumentsDelta(_)
            | E::ResponseMcpCallArgumentsDone(_)
            | E::ResponseMcpCallInProgress(_)
            | E::ResponseMcpCallCompleted(_)
            | E::ResponseMcpCallFailed(_)
            | E::ResponseMcpListToolsInProgress(_)
            | E::ResponseMcpListToolsCompleted(_)
            | E::ResponseMcpListToolsFailed(_) => Ok(Vec::new()),
        }
    }
}
