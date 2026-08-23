use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::State;
use super::items::required;

impl State {
    pub(super) fn tool_delta(
        &mut self,
        event: openai::KnownResponseStreamEvent,
        custom: bool,
    ) -> Result<Vec<Bytes>, TransformError> {
        let id = required(event.item_id, "item_id")?;
        let delta = required(event.delta, "delta")?;
        let call = self.calls.get_mut(&id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool delta before output item")
        })?;
        if call.custom != custom {
            return Err(TransformError::shape(
                "Responses stream",
                "tool delta kind changed",
            ));
        }
        call.arguments.push_str(&delta);
        Ok(Vec::new())
    }

    pub(super) fn tool_done(
        &mut self,
        mut event: openai::KnownResponseStreamEvent,
        custom: bool,
    ) -> Result<Vec<Bytes>, TransformError> {
        let id = required(event.item_id.take(), "item_id")?;
        let mut call = self.calls.remove(&id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool done before output item")
        })?;
        if call.custom != custom {
            return Err(TransformError::shape(
                "Responses stream",
                "tool done kind changed",
            ));
        }
        call.arguments = if custom {
            required(event.input, "input")?
        } else {
            required(event.arguments, "arguments")?
        };
        let item = if custom {
            openai::TypedResponseItem::CustomToolCall {
                call_id: call.call_id,
                input: call.arguments,
                name: call.name,
                id: Some(id.clone()),
                caller: None,
                namespace: None,
                rest: call.rest,
            }
        } else {
            openai::TypedResponseItem::FunctionCall {
                arguments: call.arguments,
                call_id: call.call_id,
                name: call.name,
                id: Some(id.clone()),
                caller: None,
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: call.rest,
            }
        };
        self.emit_item(openai::ResponseItem::Typed(Box::new(item)), id)
    }
}
