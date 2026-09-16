use super::{
    claude_to_responses::{Block, ClaudeToResponsesStream},
    common::{id, invalid, item_id, limit},
};
use crate::{
    transform::{
        TransformError,
        identity::{IdentityRole, OutputItemKind},
    },
    wire::{
        claude::{content as cc, generate_content as c, stream as cs},
        openai::responses::{input as i, response as r, stream as s},
    },
};
impl ClaudeToResponsesStream {
    pub(super) fn start_block(
        &mut self,
        event: cs::ContentBlockStartEvent,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        if self.blocks.len() >= self.limits.max_blocks {
            return Err(limit());
        }
        let index = event.index;
        let mut deferred = false;
        let source_index = u64::try_from(index).map_err(|_| invalid("negative block index"))?;
        let (mut item, initial) = match event.content_block {
            c::ResponseContentBlock::Text(v) => {
                let id = id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    IdentityRole::Message,
                    IdentityRole::OutputItem(OutputItemKind::Message),
                    None,
                    source_index,
                )?;
                (
                    Some(r::ResponseOutputItem::Message(i::ResponseOutputMessage {
                        id,
                        content: Vec::new(),
                        role: i::OutputMessageRole::Assistant,
                        status: i::OutputMessageStatus::InProgress,
                        phase: None,
                        type_: i::MessageType::Message,
                        rest: Default::default(),
                    })),
                    Some(v.text),
                )
            }
            c::ResponseContentBlock::Thinking(v) => {
                let id = id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    IdentityRole::Message,
                    IdentityRole::OutputItem(OutputItemKind::Reasoning),
                    None,
                    source_index,
                )?;
                (
                    Some(r::ResponseOutputItem::Reasoning(i::ReasoningItem {
                        type_: i::ReasoningItemType::ReasoningItem,
                        id,
                        summary: Vec::new(),
                        content: Some(Vec::new()),
                        status: Some(i::ReasoningStatus::InProgress),
                        encrypted_content: None,
                        rest: Default::default(),
                    })),
                    Some(v.thinking),
                )
            }
            c::ResponseContentBlock::ToolUse(v) => {
                self.count_tool()?;
                if v.name.is_empty() {
                    return Err(invalid("empty tool name"));
                }
                deferred = self.client_tools.kind(&v.name) != OutputItemKind::FunctionCall;
                let caller = v.caller.map(|caller| {
                    caller.and_then(|caller| match caller {
                        cc::Caller::Direct(_) => Some(i::Caller::Direct(i::DirectCaller {
                            rest: Default::default(),
                        })),
                        _ => None,
                    })
                });
                let call_id = id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    Some(v.id.clone()),
                    source_index,
                )?;
                let item_id = id(
                    &mut self.flow,
                    &self.policy,
                    crate::Dialect::Claude,
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(self.client_tools.kind(&v.name)),
                    Some(v.id),
                    source_index,
                )?;
                let initial = (!v.input.is_empty())
                    .then(|| serde_json::to_string(&v.input))
                    .transpose()?;
                (
                    Some(r::ResponseOutputItem::FunctionCall(i::FunctionCall {
                        type_: i::FunctionCallType::FunctionCall,
                        arguments: String::new(),
                        call_id,
                        name: v.name,
                        id: Some(item_id),
                        namespace: None,
                        caller,
                        status: Some(i::ItemStatus::InProgress),
                        rest: Default::default(),
                    })),
                    initial,
                )
            }
            c::ResponseContentBlock::McpToolUse(v) => {
                self.count_tool()?;
                if self.mcp_calls.insert(v.id.clone(), index).is_some() {
                    return Err(invalid("duplicate MCP call"));
                }
                let nonempty = !v.input.is_empty();
                let mut call = super::super::response::mcp::call(
                    v,
                    source_index,
                    &mut self.flow,
                    &self.policy,
                )?;
                let args = std::mem::take(&mut call.arguments);
                call.status = Some(i::McpCallStatus::InProgress);
                (
                    Some(r::ResponseOutputItem::McpCall(call)),
                    nonempty.then_some(args),
                )
            }
            c::ResponseContentBlock::McpToolResult(v) => {
                if !self.mcp_calls.contains_key(&v.tool_use_id) {
                    return Err(TransformError::missing_metadata("MCP call for result"));
                }
                if !self.mcp_results.insert(v.tool_use_id) {
                    return Err(invalid("duplicate MCP result"));
                }
                (None, None)
            }
            c::ResponseContentBlock::RedactedThinking(_) => (None, None),
            _ => (None, None),
        };
        if !deferred {
            item = item
                .map(|value| match value {
                    r::ResponseOutputItem::FunctionCall(call) => self.client_tools.restore(call),
                    other => Ok(other),
                })
                .map(crate::transform::optional)
                .transpose()?
                .flatten();
        }
        let output_index = if let Some(value) = &mut item {
            if self.output_items >= self.limits.max_items {
                return Err(limit());
            }
            let n = i64::try_from(self.output_items).map_err(|_| limit())?;
            self.output_items += 1;
            if !deferred {
                self.events.emit(&mut self.budget, out, |sequence_number| {
                    s::StreamEvent::OutputItemAdded(s::OutputItemEvent {
                        sequence_number,
                        output_index: n,
                        item: value.clone(),
                        rest: Default::default(),
                    })
                })?;
            }
            if matches!(
                value,
                r::ResponseOutputItem::Message(_) | r::ResponseOutputItem::Reasoning(_)
            ) {
                if self.parts >= self.limits.max_parts {
                    return Err(limit());
                }
                self.parts += 1;
                self.events.part_added(
                    &mut self.budget,
                    out,
                    n,
                    item_id(value).unwrap().to_owned(),
                    matches!(value, r::ResponseOutputItem::Reasoning(_)),
                )?;
            }
            Some(n)
        } else {
            None
        };
        self.blocks.insert(
            index,
            Block {
                output_index,
                item,
                closed: false,
                deferred,
            },
        );
        if deferred {
            self.deferred_block = Some(index);
        }
        if let Some(initial) = initial
            && !initial.is_empty()
        {
            self.payload(index, initial, out)?;
        }
        Ok(())
    }
    pub(super) fn payload(
        &mut self,
        index: i64,
        value: String,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        let block = self
            .blocks
            .get_mut(&index)
            .ok_or_else(|| invalid("missing block"))?;
        let Some(output) = block.output_index else {
            return Ok(());
        };
        let item = block.item.as_mut().unwrap();
        let id = item_id(item).unwrap().to_owned();
        let (arguments, mcp, reasoning) = match item {
            r::ResponseOutputItem::FunctionCall(v) => (Some(&mut v.arguments), false, false),
            r::ResponseOutputItem::McpCall(v) => (Some(&mut v.arguments), true, false),
            r::ResponseOutputItem::Reasoning(_) => (None, false, true),
            r::ResponseOutputItem::Message(_) => (None, false, false),
            _ => return Err(invalid("unsupported payload")),
        };
        if let Some(args) = arguments {
            let pending = self
                .argument_bytes
                .checked_add(value.len())
                .filter(|n| *n <= self.limits.max_pending.saturating_sub(self.deferred_bytes))
                .ok_or_else(limit)?;
            args.push_str(&value);
            self.argument_bytes = pending;
            if block.deferred {
                return Ok(());
            }
            self.events
                .arguments(&mut self.budget, out, output, id, value, mcp)
        } else {
            self.events
                .text(&mut self.budget, out, output, id, value, reasoning)
        }
    }
    pub(super) fn stop_block(
        &mut self,
        index: i64,
        out: &mut Vec<s::StreamEvent>,
    ) -> Result<(), TransformError> {
        let empty = self
            .blocks
            .get(&index)
            .and_then(|v| v.item.as_ref())
            .is_some_and(|v| match v {
                r::ResponseOutputItem::FunctionCall(v) => v.arguments.is_empty(),
                r::ResponseOutputItem::McpCall(v) => v.arguments.is_empty(),
                _ => false,
            });
        if empty {
            self.payload(index, "{}".into(), out)?;
        }
        let block = self
            .blocks
            .get_mut(&index)
            .ok_or_else(|| invalid("missing block stop"))?;
        block.closed = true;
        if block.deferred {
            let Some(r::ResponseOutputItem::FunctionCall(call)) = block.item.take() else {
                return Err(invalid("deferred tool lost its argument state"));
            };
            let item = self.client_tools.restore(call)?;
            self.events.emit(&mut self.budget, out, |sequence_number| {
                s::StreamEvent::OutputItemAdded(s::OutputItemEvent {
                    sequence_number,
                    output_index: block.output_index.unwrap(),
                    item: item.clone(),
                    rest: Default::default(),
                })
            })?;
            block.item = Some(item);
            self.deferred_block = None;
            return Ok(());
        }
        if let Some(item) = &block.item
            && matches!(
                item,
                r::ResponseOutputItem::FunctionCall(_) | r::ResponseOutputItem::McpCall(_)
            )
        {
            self.events
                .arguments_done(&mut self.budget, out, block.output_index.unwrap(), item)?;
        }
        Ok(())
    }
    fn count_tool(&mut self) -> Result<(), TransformError> {
        if self.tool_count >= self.limits.max_tools {
            return Err(limit());
        }
        self.tool_count += 1;
        Ok(())
    }
}
