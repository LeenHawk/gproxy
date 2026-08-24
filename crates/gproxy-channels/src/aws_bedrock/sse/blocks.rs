use gproxy_channel_api::{ChannelError, Frame};
use gproxy_protocol::aws::{ContentBlockDelta, ContentBlockStart, ReasoningContentBlockDelta};
use serde_json::json;

use super::events::{ActiveBlock, State, decode};
use super::wire::{self, BlockKind};

impl State {
    pub(super) fn block_start(
        &mut self,
        index: u64,
        start: ContentBlockStart,
    ) -> Result<Vec<Frame>, ChannelError> {
        if self.blocks.contains_key(&index) {
            return Err(decode("duplicate contentBlockStart"));
        }
        let (active, kind) = match start {
            ContentBlockStart::ToolUse { tool_use, .. } => (
                ActiveBlock::Tool,
                BlockKind::Tool {
                    id: tool_use.tool_use_id,
                    name: tool_use.name,
                },
            ),
            ContentBlockStart::ReasoningContent { .. } => {
                (ActiveBlock::Thinking, BlockKind::Thinking)
            }
            ContentBlockStart::Raw(value)
                if value.as_object().is_some_and(|map| map.is_empty()) =>
            {
                (ActiveBlock::Text, BlockKind::Text)
            }
            _ => return Err(decode("unsupported contentBlockStart")),
        };
        self.blocks.insert(index, active);
        let mut output = Vec::new();
        self.ensure_started(&mut output)?;
        output.push(wire::block_start(index, kind)?);
        Ok(output)
    }

    pub(super) fn block_delta(
        &mut self,
        index: u64,
        delta: ContentBlockDelta,
    ) -> Result<Vec<Frame>, ChannelError> {
        let (active, mapped) = match delta {
            ContentBlockDelta::Text { text, .. } => {
                (ActiveBlock::Text, json!({"type":"text_delta","text":text}))
            }
            ContentBlockDelta::ToolUse { tool_use, .. } => (
                ActiveBlock::Tool,
                json!({"type":"input_json_delta","partial_json":tool_use.input}),
            ),
            ContentBlockDelta::ReasoningContent {
                reasoning_content, ..
            } => match reasoning_content {
                ReasoningContentBlockDelta::Text { text, .. } => (
                    ActiveBlock::Thinking,
                    json!({"type":"thinking_delta","thinking":text}),
                ),
                ReasoningContentBlockDelta::Signature { signature, .. } => (
                    ActiveBlock::Thinking,
                    json!({"type":"signature_delta","signature":signature}),
                ),
                _ => return Err(decode("unsupported reasoning delta")),
            },
            _ => return Err(decode("unsupported contentBlockDelta")),
        };
        let mut output = Vec::new();
        match self.blocks.get(&index).copied() {
            Some(current)
                if std::mem::discriminant(&current) != std::mem::discriminant(&active) =>
            {
                return Err(decode("content block changed type"));
            }
            Some(_) => {}
            None if matches!(active, ActiveBlock::Tool) => {
                return Err(decode("tool delta arrived before tool start"));
            }
            None => {
                self.blocks.insert(index, active);
                self.ensure_started(&mut output)?;
                output.push(wire::block_start(
                    index,
                    if matches!(active, ActiveBlock::Thinking) {
                        BlockKind::Thinking
                    } else {
                        BlockKind::Text
                    },
                )?);
            }
        }
        output.push(wire::block_delta(index, mapped)?);
        Ok(output)
    }

    pub(super) fn block_stop(&mut self, index: u64) -> Result<Vec<Frame>, ChannelError> {
        self.blocks
            .remove(&index)
            .ok_or_else(|| decode("contentBlockStop has no open block"))?;
        Ok(vec![wire::block_stop(index)?])
    }
}
