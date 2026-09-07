use gproxy_protocol::openai;

use crate::TransformError;

use super::State;
use super::events::emit;

impl State {
    pub(super) fn stop(&mut self) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        if self.stopped {
            return Ok(Vec::new());
        }
        if self.id.is_none() || self.finish_reason.is_none() {
            return Err(TransformError::IncompleteStream);
        }
        let finish_reason = self
            .finish_reason
            .clone()
            .unwrap_or(openai::ChatFinishReason::Stop);
        let status = match finish_reason {
            openai::ChatFinishReason::Length | openai::ChatFinishReason::ContentFilter => {
                openai::ResponseStatus::Incomplete
            }
            openai::ChatFinishReason::Stop
            | openai::ChatFinishReason::ToolCalls
            | openai::ChatFinishReason::FunctionCall
            | openai::ChatFinishReason::Unknown(_) => openai::ResponseStatus::Completed,
        };
        let mut output = self.finish_items()?;
        let response = self.response(status.clone())?;
        let event = crate::wire!(openai::ResponseLifecycleEvent {
            response: Box::new(response),
            sequence_number: Some(self.next_sequence()),
            rest: Default::default(),
        });
        self.stopped = true;
        output.push(emit(match status {
            openai::ResponseStatus::Incomplete => {
                openai::KnownResponseStreamEvent::ResponseIncomplete(event)
            }
            openai::ResponseStatus::Completed => {
                openai::KnownResponseStreamEvent::ResponseCompleted(event)
            }
            openai::ResponseStatus::Failed
            | openai::ResponseStatus::InProgress
            | openai::ResponseStatus::Cancelled
            | openai::ResponseStatus::Queued
            | openai::ResponseStatus::Unknown(_) => {
                openai::KnownResponseStreamEvent::ResponseCompleted(event)
            }
        })?);
        Ok(output)
    }
}
