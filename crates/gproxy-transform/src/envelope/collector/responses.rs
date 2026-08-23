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
            openai::ResponseStreamEvent::Known(event)
                if matches!(
                    event.type_,
                    openai::ResponseStreamEventTypeKnown::ResponseCompleted
                        | openai::ResponseStreamEventTypeKnown::ResponseIncomplete
                        | openai::ResponseStreamEventTypeKnown::ResponseFailed
                ) =>
            {
                self.response = event.response;
            }
            openai::ResponseStreamEvent::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Responses stream event",
                    raw.to_string(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<openai::ResponseObject, TransformError> {
        self.response
            .map(|response| *response)
            .ok_or(TransformError::IncompleteStream)
    }
}
