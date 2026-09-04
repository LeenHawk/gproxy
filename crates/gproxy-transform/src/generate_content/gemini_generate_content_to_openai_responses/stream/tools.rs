use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::State;

impl State {
    pub(super) fn tool_delta(
        &mut self,
        event: openai::ResponseItemStringDeltaEvent,
        custom: bool,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        let call = self.calls.get_mut(&event.item_id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool delta before output item")
        })?;
        if call.custom != custom {
            return Err(TransformError::shape(
                "Responses stream",
                "tool delta kind changed",
            ));
        }
        call.arguments.push_str(&event.delta);
        Ok(Vec::new())
    }

    pub(super) fn function_done(
        &mut self,
        event: openai::ResponseFunctionCallArgumentsDoneEvent,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        let id = event
            .item_id
            .or_else(|| self.call_indices.remove(&event.output_index))
            .ok_or_else(|| TransformError::shape("Responses stream", "tool item id missing"))?;
        self.finish_tool(id, event.output_index, event.arguments, false)
    }

    pub(super) fn custom_done(
        &mut self,
        event: openai::ResponseCustomToolCallInputDoneEvent,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        self.finish_tool(event.item_id, event.output_index, event.input, true)
    }

    fn finish_tool(
        &mut self,
        id: String,
        output_index: u32,
        input: String,
        custom: bool,
    ) -> Result<Vec<gemini::GenerateContentResponse>, TransformError> {
        self.call_indices.remove(&output_index);
        let mut call = self.calls.remove(&id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool done before output item")
        })?;
        if call.custom != custom {
            return Err(TransformError::shape(
                "Responses stream",
                "tool done kind changed",
            ));
        }
        call.arguments = input;
        let item = if custom {
            openai::TypedResponseItem::CustomToolCall {
                call_id: call.call_id,
                input: call.arguments,
                name: call.name,
                id: Some(id.clone()),
                caller: None,
                namespace: None,
                async_: None,
                rest: Default::default(),
            }
        } else {
            openai::TypedResponseItem::FunctionCall {
                arguments: call.arguments,
                call_id: call.call_id,
                name: call.name,
                id: Some(id.clone()),
                caller: None,
                namespace: None,
                async_: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            }
        };
        self.emit_item(openai::ResponseItem::Typed(Box::new(item)), id)
    }
}
