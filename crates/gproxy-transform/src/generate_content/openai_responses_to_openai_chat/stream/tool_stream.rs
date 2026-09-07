use gproxy_protocol::openai;

use crate::TransformError;

use super::events::{emit, tool_item};
use super::tools::{tool_kind, tool_kind_or, tool_metadata, tool_payload};
use super::{State, Tool, ToolKind};

impl State {
    pub(super) fn tool_delta(
        &mut self,
        call: openai::ChatToolCallDelta,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let mut output = self.finish_scalar()?;
        let chat_index = call.index;
        if !self.tools.contains_key(&chat_index) {
            let id = call.id.clone().ok_or_else(|| {
                TransformError::shape("Chat stream", "tool call id missing on first delta")
            })?;
            let kind = tool_kind(&call)?;
            let name = tool_metadata(&call, kind)?;
            let item = Tool {
                id,
                index: self.allocate(),
                name,
                arguments: String::new(),
                kind,
            };
            output.push(emit(
                openai::KnownResponseStreamEvent::ResponseOutputItemAdded(crate::wire!(
                    openai::ResponseOutputItemEvent {
                        item: Box::new(tool_item(
                            &item,
                            openai::ResponseItemLifecycleStatus::InProgress,
                        )),
                        output_index: item.index,
                        sequence_number: Some(self.next_sequence()),
                        rest: Default::default(),
                    }
                )),
            )?);
            self.tools.insert(chat_index, item);
        }
        let item = self.tools.get(&chat_index).expect("created");
        if call.id.as_ref().is_some_and(|id| id != &item.id) {
            return Err(TransformError::shape(
                "Chat stream",
                "tool call id changed between deltas",
            ));
        }
        let kind = tool_kind_or(&call, item.kind)?;
        if kind != item.kind {
            return Err(TransformError::shape(
                "Chat stream",
                "tool call kind changed between deltas",
            ));
        }
        let (delta, name) = tool_payload(call, kind)?;
        let (id, output_index) = {
            let item = self.tools.get_mut(&chat_index).expect("created");
            if name.as_ref().is_some_and(|name| name != &item.name) {
                return Err(TransformError::shape(
                    "Chat stream",
                    "tool call name changed between deltas",
                ));
            }
            item.arguments.push_str(&delta);
            (item.id.clone(), item.index)
        };
        if !delta.is_empty() {
            let payload = crate::wire!(openai::ResponseItemStringDeltaEvent {
                delta,
                item_id: id,
                output_index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            });
            output.push(emit(match kind {
                ToolKind::Function => {
                    openai::KnownResponseStreamEvent::ResponseFunctionCallArgumentsDelta(payload)
                }
                ToolKind::Custom => {
                    openai::KnownResponseStreamEvent::ResponseCustomToolCallInputDelta(payload)
                }
            })?);
        }
        Ok(output)
    }
}
