use bytes::Bytes;

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

use super::state::State;

impl Converter for State {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        self.push_typed(serde_json::from_str(&frame.data)?)?
            .into_iter()
            .map(|event| SseFrame::typed(None, &event))
            .collect()
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        self.finish_typed().map(|_| Vec::new())
    }
}
