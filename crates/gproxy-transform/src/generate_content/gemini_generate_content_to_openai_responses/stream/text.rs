use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::items::required;
use super::{State, events};

impl State {
    pub(super) fn text_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
        thought: bool,
    ) -> Result<Vec<Bytes>, TransformError> {
        let text = required(event.delta, "delta")?;
        if let Some(id) = event.item_id {
            self.text_items.insert(id);
        }
        let chunk = events::chunk(
            Some(events::text(text, thought)),
            None,
            None,
            self.response_id.clone(),
            self.model.clone(),
        );
        Ok(vec![self.emit(chunk)?])
    }

    pub(super) fn refusal_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let text = required(event.delta, "delta")?;
        if let Some(id) = event.item_id {
            self.text_items.insert(id);
        }
        let chunk = events::chunk(
            Some(events::text(text, false)),
            None,
            None,
            self.response_id.clone(),
            self.model.clone(),
        );
        Ok(vec![self.emit(chunk)?])
    }
}
