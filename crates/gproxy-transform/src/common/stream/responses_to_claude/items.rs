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
}

impl State {
    pub(super) fn response_output_item_added(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        let native_id = match event.item.as_ref() {
            openai::ResponseItem::Typed(item) => items::item_id(item),
            openai::ResponseItem::Message(_) | openai::ResponseItem::Unknown(_) => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        };
        let item_id = item_id(&event.item).or(native_id);
        match *event.item {
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                    ..
                } => {
                    output.extend(self.response_tool(ToolStart {
                        item_id: item_id.ok_or_else(|| {
                            TransformError::shape("Responses stream", "function item id is missing")
                        })?,
                        id: call_id,
                        name,
                        input: arguments,
                        output_index: event.output_index,
                    })?);
                }
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    name,
                    input,
                    ..
                } => {
                    output.extend(self.response_tool(ToolStart {
                        item_id: item_id.ok_or_else(|| {
                            TransformError::shape("Responses stream", "custom item id is missing")
                        })?,
                        id: call_id,
                        name,
                        input,
                        output_index: event.output_index,
                    })?);
                }
                openai::TypedResponseItem::Reasoning {
                    encrypted_content, ..
                } => {
                    let item_id = item_id.ok_or_else(|| {
                        TransformError::shape("Responses stream", "reasoning item id is missing")
                    })?;
                    let index = self.allocate();
                    output.extend(self.block_start(
                        index,
                        claude::ResponseContentBlock::Thinking(crate::wire!(
                            claude::ThinkingBlock {
                                signature: encrypted_content,
                                thinking: String::new(),
                                type_: claude::ThinkingBlockType::Thinking,
                                rest: Default::default(),
                            }
                        )),
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
                        output.extend(self.block_start(index, items::response_block(call))?);
                        if let Some(item_id) = item_id {
                            self.response_indices.insert((item_id, None), index);
                        }
                        self.response_output_indices
                            .entry(event.output_index)
                            .or_default()
                            .push(index);
                        self.has_tool = true;
                    } else {
                        return Ok(output);
                    }
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            },
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                if !message.content.is_empty() {
                    return Ok(output);
                }
            }
            openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_))
            | openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_))
            | openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(_))
            | openai::ResponseItem::Unknown(_) => return Ok(output),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
        Ok(output)
    }

    fn response_tool(
        &mut self,
        start: ToolStart,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let ToolStart {
            item_id,
            id,
            name,
            input,
            output_index,
        } = start;
        let index = self.allocate();
        let mut output = self.block_start(
            index,
            claude::ResponseContentBlock::ToolUse(crate::wire!(claude::ResponseToolUseBlock {
                id,
                input: Default::default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: Default::default(),
            })),
        )?;
        self.response_indices.insert((item_id, None), index);
        self.response_tool_inputs.insert(index, input.clone());
        self.response_output_indices
            .entry(output_index)
            .or_default()
            .push(index);
        self.has_tool = true;
        if !input.is_empty() {
            output.push(self.input_delta(index, input)?);
        }
        Ok(output)
    }
}
