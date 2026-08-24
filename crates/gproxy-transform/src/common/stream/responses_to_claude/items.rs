use bytes::Bytes;
use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;

use super::State;
use super::item_id;

struct ToolStart {
    item_id: String,
    id: String,
    name: String,
    input: String,
    output_index: u32,
    rest: openai::Rest,
    event_rest: openai::Rest,
}

impl State {
    pub(super) fn response_output_item_added(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = self.ensure_start(Default::default(), event.rest.clone())?;
        let native_id = match event.item.as_ref() {
            openai::ResponseItem::Typed(item) => items::item_id(item),
            openai::ResponseItem::Message(_) | openai::ResponseItem::Unknown(_) => None,
        };
        let item_id = item_id(&event.item).or(native_id);
        match *event.item {
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    id,
                    call_id,
                    name,
                    arguments,
                    mut rest,
                    ..
                } => {
                    if let Some(id) = id {
                        rest.insert("openai_item_id".into(), id.into());
                    }
                    output.extend(self.response_tool(ToolStart {
                        item_id: item_id.ok_or_else(|| {
                            TransformError::shape("Responses stream", "function item id is missing")
                        })?,
                        id: call_id,
                        name,
                        input: arguments,
                        output_index: event.output_index,
                        rest,
                        event_rest: event.rest,
                    })?);
                }
                openai::TypedResponseItem::CustomToolCall {
                    id,
                    call_id,
                    name,
                    input,
                    mut rest,
                    ..
                } => {
                    if let Some(id) = id {
                        rest.insert("openai_item_id".into(), id.into());
                    }
                    output.extend(self.response_tool(ToolStart {
                        item_id: item_id.ok_or_else(|| {
                            TransformError::shape("Responses stream", "custom item id is missing")
                        })?,
                        id: call_id,
                        name,
                        input,
                        output_index: event.output_index,
                        rest,
                        event_rest: event.rest,
                    })?);
                }
                openai::TypedResponseItem::Reasoning {
                    encrypted_content,
                    rest,
                    ..
                } => {
                    let item_id = item_id.ok_or_else(|| {
                        TransformError::shape("Responses stream", "reasoning item id is missing")
                    })?;
                    let index = self.allocate();
                    output.extend(self.block_start(
                        index,
                        claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                            signature: encrypted_content,
                            thinking: String::new(),
                            type_: claude::ThinkingBlockType::Thinking,
                            rest,
                        }),
                        event.rest,
                    )?);
                    self.response_indices.insert((item_id, None), index);
                }
                other @ (openai::TypedResponseItem::FileSearchCall { .. }
                | openai::TypedResponseItem::ComputerCall { .. }
                | openai::TypedResponseItem::ComputerCallOutput { .. }
                | openai::TypedResponseItem::WebSearchCall { .. }
                | openai::TypedResponseItem::FunctionCallOutput { .. }
                | openai::TypedResponseItem::ToolSearchCall { .. }
                | openai::TypedResponseItem::ToolSearchOutput { .. }
                | openai::TypedResponseItem::AdditionalTools { .. }
                | openai::TypedResponseItem::Compaction { .. }
                | openai::TypedResponseItem::ImageGenerationCall { .. }
                | openai::TypedResponseItem::CodeInterpreterCall { .. }
                | openai::TypedResponseItem::LocalShellCall { .. }
                | openai::TypedResponseItem::LocalShellCallOutput { .. }
                | openai::TypedResponseItem::ShellCall { .. }
                | openai::TypedResponseItem::ShellCallOutput { .. }
                | openai::TypedResponseItem::ApplyPatchCall { .. }
                | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
                | openai::TypedResponseItem::McpListTools { .. }
                | openai::TypedResponseItem::McpApprovalRequest { .. }
                | openai::TypedResponseItem::McpApprovalResponse { .. }
                | openai::TypedResponseItem::McpCall { .. }
                | openai::TypedResponseItem::CustomToolCallOutput { .. }
                | openai::TypedResponseItem::Program { .. }
                | openai::TypedResponseItem::ProgramOutput { .. }
                | openai::TypedResponseItem::MultiAgentCall { .. }
                | openai::TypedResponseItem::MultiAgentCallOutput { .. }
                | openai::TypedResponseItem::AgentMessage { .. }
                | openai::TypedResponseItem::CompactionTrigger { .. }
                | openai::TypedResponseItem::ItemReference { .. }) => {
                    if let Some(call) = items::openai_call(other.clone())? {
                        let index = self.allocate();
                        output.extend(self.block_start(
                            index,
                            items::response_block(call),
                            event.rest,
                        )?);
                        if let Some(item_id) = item_id {
                            self.response_indices.insert((item_id, None), index);
                        }
                        self.response_output_indices
                            .entry(event.output_index)
                            .or_default()
                            .push(index);
                        self.has_tool = true;
                    } else {
                        return Err(TransformError::unsupported(
                            "Responses output item",
                            serde_json::to_string(&other)?,
                        ));
                    }
                }
            },
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                if !message.content.is_empty() {
                    return Err(TransformError::unsupported(
                        "Responses output item",
                        "message content before content-part events",
                    ));
                }
            }
            openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_)) => {
                return Err(TransformError::unsupported(
                    "Responses output item",
                    "input message in output stream",
                ));
            }
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_)) => {
                return Err(TransformError::unsupported(
                    "Responses output item",
                    "easy input message in output stream",
                ));
            }
            openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(raw)) => {
                return Err(TransformError::unsupported(
                    "Responses output message",
                    raw.to_string(),
                ));
            }
            openai::ResponseItem::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Responses output item",
                    raw.to_string(),
                ));
            }
        }
        Ok(output)
    }

    fn response_tool(&mut self, start: ToolStart) -> Result<Vec<Bytes>, TransformError> {
        let ToolStart {
            item_id,
            id,
            name,
            input,
            output_index,
            rest,
            event_rest,
        } = start;
        let index = self.allocate();
        let mut output = self.block_start(
            index,
            claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id,
                input: Default::default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest,
            }),
            event_rest,
        )?;
        self.response_indices.insert((item_id, None), index);
        self.response_output_indices
            .entry(output_index)
            .or_default()
            .push(index);
        self.has_tool = true;
        if !input.is_empty() {
            output.push(self.input_delta(index, input, Default::default())?);
        }
        Ok(output)
    }
}
