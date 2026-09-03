use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::State;
use super::wire::empty_delta;

impl State {
    pub(super) fn text_delta(
        &mut self,
        event: openai::ResponseOutputTextDeltaEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.text.push_str(&event.delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                content: Some(event.delta),
                ..empty_delta()
            },
            None,
            None,
        )?])
    }

    pub(super) fn reasoning_text_delta(
        &mut self,
        event: openai::ResponseContentDeltaEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.reasoning_delta(event.delta)
    }

    pub(super) fn reasoning_summary_delta(
        &mut self,
        event: openai::ResponseReasoningSummaryTextDeltaEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.reasoning_delta(event.delta)
    }

    fn reasoning_delta(&mut self, delta: String) -> Result<Vec<Bytes>, TransformError> {
        self.reasoning.push_str(&delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                reasoning_content: Some(delta),
                ..empty_delta()
            },
            None,
            None,
        )?])
    }

    pub(super) fn refusal_delta(
        &mut self,
        event: openai::ResponseContentDeltaEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.refusal.push_str(&event.delta);
        Ok(vec![self.chunk(
            openai::ChatDelta {
                refusal: Some(event.delta),
                ..empty_delta()
            },
            None,
            None,
        )?])
    }

    pub(super) fn reasoning_part_added(
        &mut self,
        event: openai::ResponseReasoningSummaryPartAddedEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.finish_reasoning(event.part.text)
    }

    pub(super) fn reasoning_part_done(
        &mut self,
        event: openai::ResponseReasoningSummaryPartDoneEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        self.finish_reasoning(event.part.text)
    }
}
