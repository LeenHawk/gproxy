use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::State;
use super::item_id;

impl State {
    pub(super) fn response_output_item_done(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = self.ensure_start(Default::default(), event.rest.clone())?;
        let id = item_id(&event.item);
        let indices = if let Some(id) = id {
            self.response_indices
                .iter()
                .filter_map(|((candidate, _), index)| (candidate == &id).then_some(*index))
                .collect::<Vec<_>>()
        } else {
            self.response_output_indices
                .remove(&event.output_index)
                .unwrap_or_default()
        };
        if indices.is_empty() {
            return Ok(output);
        }
        for (position, index) in indices.iter().enumerate() {
            let rest = if position + 1 == indices.len() {
                event.rest.clone()
            } else {
                Default::default()
            };
            output.extend(self.close(*index, rest)?);
        }
        Ok(output)
    }
}
