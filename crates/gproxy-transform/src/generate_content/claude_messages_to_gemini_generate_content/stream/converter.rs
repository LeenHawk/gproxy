use bytes::Bytes;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

use super::state::State;

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.event(serde_json::from_str(&frame.data)?)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        if self.started && self.saw_finish && self.stopped && self.tools.is_empty() {
            Ok(Vec::new())
        } else {
            Err(TransformError::IncompleteStream)
        }
    }
}
