mod content_events;
mod items;
mod lifecycle;
mod native;
mod state;
mod tool_events;
mod wire;

use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

use state::{State, Tool, ToolKind, ToolStart};

pub(crate) fn converter() -> Box<dyn Converter> {
    Box::new(State::default())
}

impl State {
    /// An event we do not render is not a reason to kill a response that is already
    /// arriving. Vendors add stream events continuously and emit tool-surface events
    /// (search, MCP, code interpreter) that Chat Completions has no place to put, so
    /// anything unrenderable is dropped and the stream continues. The list stays
    /// exhaustive: adding a variant still forces a decision here, it just no longer
    /// defaults to hanging up on the caller.
    fn event(&mut self, event: openai::ResponseStreamEvent) -> Result<Vec<Bytes>, TransformError> {
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Ok(Vec::new());
        };
        match *event {
            openai::KnownResponseStreamEvent::ResponseCreated(event)
            | openai::KnownResponseStreamEvent::ResponseInProgress(event) => self.start(event),
            openai::KnownResponseStreamEvent::ResponseCompleted(event) => {
                self.terminal(event, openai::ResponseStatus::Completed)
            }
            openai::KnownResponseStreamEvent::ResponseIncomplete(event) => {
                self.terminal(event, openai::ResponseStatus::Incomplete)
            }
            openai::KnownResponseStreamEvent::ResponseFailed(_)
            | openai::KnownResponseStreamEvent::Error(_) => Err(TransformError::unsupported(
                "Responses stream",
                "failed response",
            )),
            openai::KnownResponseStreamEvent::ResponseOutputItemAdded(event)
            | openai::KnownResponseStreamEvent::ResponseOutputItemDone(event) => {
                self.complete_item(*event.item, event.output_index, event.rest)
            }
            openai::KnownResponseStreamEvent::ResponseContentPartAdded(event)
            | openai::KnownResponseStreamEvent::ResponseContentPartDone(event) => self
                .complete_part(
                    event.part,
                    event.item_id,
                    event.output_index,
                    event.content_index,
                    event.rest,
                ),
            openai::KnownResponseStreamEvent::ResponseOutputTextDelta(event) => {
                self.text_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDelta(event) => {
                self.reasoning_text_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDelta(event) => {
                self.reasoning_summary_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDelta(event) => {
                self.refusal_delta(event)
            }
            openai::KnownResponseStreamEvent::ResponseOutputTextDone(event) => {
                self.finish_text(event.text, Default::default(), event.rest)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningTextDone(event) => {
                self.finish_reasoning(event.text, Default::default(), event.rest)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryTextDone(event) => {
                self.finish_reasoning(event.text, Default::default(), event.rest)
            }
            openai::KnownResponseStreamEvent::ResponseRefusalDone(event) => {
                self.finish_refusal(event.refusal, Default::default(), event.rest)
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(event) => {
                self.tool_delta(event, ToolKind::Function)
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta(event) => {
                self.tool_delta(event, ToolKind::Custom)
            }
            openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDone(event) => {
                self.function_done(event)
            }
            openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDone(event) => {
                self.custom_done(event)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartAdded(event) => {
                self.reasoning_part_added(event)
            }
            openai::KnownResponseStreamEvent::ResponseReasoningSummaryPartDone(event) => {
                self.reasoning_part_done(event)
            }
            openai::KnownResponseStreamEvent::ResponseQueued(_)
            | openai::KnownResponseStreamEvent::ResponseOutputTextAnnotationAdded(_)
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

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.event(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.stopped {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
