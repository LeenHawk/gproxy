use gproxy_protocol::openai;

use crate::TransformError;

use super::{State, ToolKind};

impl State {
    pub(super) fn tool_delta(
        &mut self,
        event: openai::ResponseItemStringDeltaEvent,
        kind: ToolKind,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        let tool = self.tools.get_mut(&event.item_id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool delta before output item")
        })?;
        if tool.kind != kind || tool.output_index != event.output_index {
            return Err(TransformError::shape(
                "Responses stream",
                "tool delta does not match its output item",
            ));
        }
        tool.data.push_str(&event.delta);
        let index = tool.index;
        Ok(vec![self.tool_chunk(index, kind, event.delta)?])
    }

    pub(super) fn function_done(
        &mut self,
        event: openai::ResponseFunctionCallArgumentsDoneEvent,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        let id = self.tool_source_id(event.item_id, event.output_index)?;
        self.finish_tool_event(
            &id,
            event.output_index,
            ToolKind::Function,
            event.arguments,
            event.name,
        )
    }

    pub(super) fn custom_done(
        &mut self,
        event: openai::ResponseCustomToolCallInputDoneEvent,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        self.finish_tool_event(
            &event.item_id,
            event.output_index,
            ToolKind::Custom,
            event.input,
            None,
        )
    }

    fn finish_tool_event(
        &mut self,
        id: &str,
        output_index: u32,
        kind: ToolKind,
        full: String,
        name: Option<String>,
    ) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        if let Some(name) = name {
            let tool = self.tools.get(id).ok_or_else(|| {
                TransformError::shape("Responses stream", "tool done before output item")
            })?;
            if tool.name != name {
                return Err(TransformError::shape(
                    "Responses stream",
                    "tool done name does not match its output item",
                ));
            }
        }
        self.finish_tool(id, output_index, kind, full)
    }

    fn tool_source_id(
        &self,
        item_id: Option<String>,
        output_index: u32,
    ) -> Result<String, TransformError> {
        if let Some(item_id) = item_id {
            return Ok(item_id);
        }
        let mut candidates = self
            .tools
            .iter()
            .filter_map(|(id, tool)| (tool.output_index == output_index).then_some(id));
        let id = candidates.next().ok_or_else(|| {
            TransformError::shape("Responses stream", "tool done before output item")
        })?;
        if candidates.next().is_some() {
            return Err(TransformError::shape(
                "Responses stream",
                "sparse tool done has ambiguous output index",
            ));
        }
        Ok(id.clone())
    }
}
