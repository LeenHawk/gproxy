mod content;
mod item_done;
mod item_id;
mod items;
mod terminal;

use item_id::item_id;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::SseFrame;

use super::openai_to_claude::{Scalar, State};

impl State {
    pub(super) fn responses(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let event: openai::ResponseStreamEvent = serde_json::from_str(&frame.data)?;
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Ok(Vec::new());
        };
        match *event {
            openai::KnownResponseStreamEvent::ResponseCreated(event) => {
                self.response_created(event)
            }
            openai::KnownResponseStreamEvent::ResponseInProgress(event)
            | openai::KnownResponseStreamEvent::ResponseQueued(event) => {
                self.response_pending(event)
            }
            openai::KnownResponseStreamEvent::ResponseCompleted(event) => {
                self.response_completed(event)
            }
            openai::KnownResponseStreamEvent::ResponseIncomplete(event) => {
                self.response_incomplete(event)
            }
            openai::KnownResponseStreamEvent::ResponseFailed(_)
            | openai::KnownResponseStreamEvent::Error(_) => Err(TransformError::unsupported(
                "Responses stream",
                "failed response",
            )),
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded(event) => {
                self.response_output_item_added(event)
            }
            openai::KnownResponseStreamEvent::ResponseOutputItemDone(event) => {
                self.response_output_item_done(event)
            }
            openai::KnownResponseStreamEvent::ResponseContentPartAdded(event) => {
                self.response_content_part_added(event)
            }
            openai::KnownResponseStreamEvent::ResponseContentPartDone(event) => {
                self.response_content_done(&event.item_id, Some(event.content_index))
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta(event) => {
                self.response_output_text_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(event) => {
                self.response_content_delta(event, Scalar::Thinking)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta(event) => {
                self.response_summary_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta(event) => {
                self.response_content_delta(event, Scalar::Text)
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(event)
            | openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta(event) => {
                self.response_tool_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDone(event) => {
                self.response_content_done(&event.item_id, Some(event.content_index))
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDone(event) => {
                self.response_content_done(&event.item_id, Some(event.content_index))
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDone(event) => {
                self.response_content_done(&event.item_id, None)
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDone(event) => {
                self.response_content_done(&event.item_id, Some(event.content_index))
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(event) => self
                .response_tool_done(
                    event.item_id.as_deref(),
                    event.output_index,
                    event.arguments,
                ),
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(event) => {
                self.response_tool_done(Some(&event.item_id), event.output_index, event.input)
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextAnnotationAdded(_)
            | openai::KnownResponseStreamEvent::ResponseInjectCreated(_)
            | openai::KnownResponseStreamEvent::ResponseInjectFailed(_)
            | openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartAdded(_)
            | openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartDone(_)
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
            | openai::KnownResponseStreamEvent::ResponseMcpListToolsFailed(_) => Ok(Vec::new()),
        }
    }
}
