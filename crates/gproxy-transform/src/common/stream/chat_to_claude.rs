use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};
use crate::envelope::SseFrame;
use crate::models::common::wire_string;

use super::openai_to_claude::{Scalar, State};

impl State {
    pub(super) fn chat(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        if frame.data == "[DONE]" {
            return self.stop();
        }
        let chunk: openai::ChatCompletionChunk = serde_json::from_str(&frame.data)?;
        self.id = Some(chunk.id);
        self.model = Some(wire_string(&chunk.model)?.into());
        let chunk_rest = chunk.rest;
        let mut output = self.ensure_start(Default::default(), chunk_rest.clone())?;
        if let Some(usage) = usage::chat_to_claude(chunk.usage) {
            output.extend(self.usage_delta(usage, chunk_rest.clone())?);
        }
        for choice in chunk.choices {
            if choice.index != 0 {
                return Err(TransformError::unsupported(
                    "Chat stream",
                    "multiple choices",
                ));
            }
            let mut payload_rest = chunk_rest.clone();
            payload_rest.extend(choice.rest);
            payload_rest.extend(choice.delta.rest.clone());
            if let Some(reasoning) = choice.delta.reasoning_content {
                output.extend(self.scalar_delta(
                    "thinking",
                    Scalar::Thinking,
                    reasoning,
                    payload_rest.clone(),
                )?);
            }
            if let Some(text) = choice.delta.content {
                output.extend(self.scalar_delta(
                    "text",
                    Scalar::Text,
                    text,
                    payload_rest.clone(),
                )?);
            }
            if let Some(refusal) = choice.delta.refusal {
                output.extend(self.scalar_delta(
                    "refusal",
                    Scalar::Text,
                    refusal,
                    payload_rest.clone(),
                )?);
            }
            if choice.delta.function_call.is_some() {
                return Err(TransformError::unsupported(
                    "Chat stream",
                    "legacy function_call delta",
                ));
            }
            for call in choice.delta.tool_calls.into_iter().flatten() {
                let key = format!("tool:{}", call.index);
                let mut tool_rest = payload_rest.clone();
                tool_rest.extend(call.rest);
                let (name, arguments, variant_rest) = match (call.function, call.custom) {
                    (Some(function), None) => (function.name, function.arguments, function.rest),
                    (None, Some(custom)) => (custom.name, custom.input, custom.rest),
                    (Some(_), Some(_)) => {
                        return Err(TransformError::shape(
                            "Chat stream",
                            "tool delta has both function and custom payloads",
                        ));
                    }
                    (None, None) => (None, None, Default::default()),
                };
                tool_rest.extend(variant_rest);
                let index = if let Some(index) = self.item_indices.get(&key).copied() {
                    index
                } else {
                    let index = self.allocate();
                    let id = call.id.ok_or_else(|| {
                        TransformError::shape("Chat stream", "tool start id is missing")
                    })?;
                    let name = name.clone().ok_or_else(|| {
                        TransformError::shape("Chat stream", "tool start name is missing")
                    })?;
                    output.extend(self.block_start(
                        index,
                        claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                            id,
                            input: Default::default(),
                            name,
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            rest: tool_rest.clone(),
                        }),
                        tool_rest.clone(),
                    )?);
                    self.item_indices.insert(key.clone(), index);
                    index
                };
                let arguments = arguments.unwrap_or_default();
                if !arguments.is_empty() || !tool_rest.is_empty() {
                    output.push(self.input_delta(index, arguments, tool_rest)?);
                }
            }
            if let Some(reason) = choice.finish_reason {
                output.extend(self.finish_message(
                    stop::chat_to_claude(reason),
                    None,
                    false,
                    payload_rest,
                )?);
            }
        }
        Ok(output)
    }
}
