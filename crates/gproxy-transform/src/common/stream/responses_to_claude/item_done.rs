use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::State;
use super::item_id;

impl State {
    pub(super) fn response_output_item_done(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        let tool_input = match event.item.as_ref() {
            openai::ResponseItem::Typed(item) => match item.as_ref() {
                openai::TypedResponseItem::FunctionCall { arguments, .. } => {
                    Some(arguments.clone())
                }
                openai::TypedResponseItem::CustomToolCall { input, .. } => Some(input.clone()),
                _ => None,
            },
            openai::ResponseItem::Message(_) | openai::ResponseItem::Unknown(_) => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        };
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
        if let Some(full) = tool_input {
            for index in &indices {
                if self.response_tool_inputs.contains_key(index) {
                    output.extend(self.response_tool_full(*index, full.clone())?);
                }
            }
        }
        for (position, index) in indices.iter().enumerate() {
            let _ = position;
            output.extend(self.close(*index)?);
        }
        Ok(output)
    }
}
