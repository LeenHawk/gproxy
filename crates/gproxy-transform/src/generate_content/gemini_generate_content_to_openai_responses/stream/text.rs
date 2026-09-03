use gproxy_protocol::gemini;

use crate::TransformError;

use super::{State, events};

impl State {
    pub(super) fn text_delta(
        &mut self,
        text: String,
        item_id: String,
        thought: bool,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        self.text_items.insert(item_id);
        let chunk = events::chunk(
            Some(events::text(text, thought)),
            None,
            None,
            self.response_id.clone(),
            self.model.clone(),
        );
        Ok(vec![self.emit(chunk)?])
    }
}
