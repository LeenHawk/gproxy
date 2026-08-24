use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::{State, Tool, ToolKind, ToolStart};
use super::suffix;
use crate::generate_content::openai_chat_to_openai_responses::stream::wire::empty_delta;

impl State {
    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn start_tool(
        &mut self,
        start: ToolStart,
    ) -> Result<Vec<Bytes>, TransformError> {
        let ToolStart {
            source_id,
            call_id,
            output_index,
            name,
            kind,
            rest,
            event_rest,
        } = start;
        if let Some(tool) = self.tools.get(&source_id) {
            if tool.kind != kind
                || tool.output_index != output_index
                || tool.call_id != call_id
                || tool.name != name
            {
                return Err(TransformError::shape(
                    "Responses stream",
                    "tool output item changed kind or index",
                ));
            }
            return if rest.is_empty() && event_rest.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(vec![self.preserve(rest, event_rest)?])
            };
        }
        let index = self.next_tool;
        self.next_tool += 1;
        self.tools.insert(
            source_id,
            Tool {
                index,
                output_index,
                kind,
                call_id: call_id.clone(),
                name: name.clone(),
                data: String::new(),
            },
        );
        let call = openai::ChatToolCallDelta {
            index,
            id: Some(call_id),
            type_: Some(match kind {
                ToolKind::Function => openai::ChatToolCallType::Function,
                ToolKind::Custom => openai::ChatToolCallType::Custom,
            }),
            function: (kind == ToolKind::Function).then(|| openai::FunctionCallDelta {
                arguments: None,
                name: Some(name.clone()),
                rest: Default::default(),
            }),
            custom: (kind == ToolKind::Custom).then(|| openai::CustomToolCallDelta {
                input: None,
                name: Some(name),
                rest: Default::default(),
            }),
            rest,
        };
        Ok(vec![self.chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![call]),
                ..empty_delta()
            },
            None,
            None,
            event_rest,
        )?])
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn finish_tool(
        &mut self,
        id: &str,
        output_index: u32,
        kind: ToolKind,
        full: String,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        let tool = self.tools.get_mut(id).ok_or_else(|| {
            TransformError::shape("Responses stream", "tool done before output item")
        })?;
        if tool.kind != kind || tool.output_index != output_index {
            return Err(TransformError::shape(
                "Responses stream",
                "tool done does not match its output item",
            ));
        }
        let delta = suffix(&tool.data, &full, "tool input")?;
        tool.data = full;
        let index = tool.index;
        if delta.is_empty() && event_rest.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![self.tool_chunk(index, kind, delta, event_rest)?])
        }
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn tool_chunk(
        &self,
        index: u32,
        kind: ToolKind,
        delta: String,
        rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        self.chunk(
            openai::ChatDelta {
                tool_calls: Some(vec![openai::ChatToolCallDelta {
                    index,
                    id: None,
                    type_: None,
                    function: (kind == ToolKind::Function).then(|| openai::FunctionCallDelta {
                        arguments: Some(delta.clone()),
                        name: None,
                        rest: Default::default(),
                    }),
                    custom: (kind == ToolKind::Custom).then(|| openai::CustomToolCallDelta {
                        input: Some(delta),
                        name: None,
                        rest: Default::default(),
                    }),
                    rest: Default::default(),
                }]),
                ..empty_delta()
            },
            None,
            None,
            rest,
        )
    }

    pub(super) fn preserve(
        &self,
        delta_rest: openai::Rest,
        event_rest: openai::Rest,
    ) -> Result<Bytes, TransformError> {
        let mut delta = empty_delta();
        delta.rest = delta_rest;
        self.chunk(delta, None, None, event_rest)
    }
}
