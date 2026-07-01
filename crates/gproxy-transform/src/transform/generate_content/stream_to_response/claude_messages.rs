use std::collections::BTreeMap;

use serde_json::Value;

use crate::protocol::claude;

use super::super::common;

pub fn response(
    events: impl IntoIterator<Item = claude::StreamEvent>,
) -> claude::CreateMessageResponseBody {
    let mut collector = ClaudeCollector::default();
    for event in events {
        collector.push(event);
    }
    collector.finish()
}

#[derive(Default)]
struct ClaudeCollector {
    id: Option<String>,
    type_: Option<claude::MessageObjectType>,
    role: Option<claude::AssistantRole>,
    model: Option<claude::ClaudeModel>,
    content: BTreeMap<u64, BlockState>,
    stop_reason: Option<claude::StopReason>,
    stop_sequence: Option<String>,
    usage: Option<claude::Usage>,
    container: Option<claude::Container>,
    context_management: Option<claude::ContextManagementResponse>,
    stop_details: Option<claude::StopDetails>,
    errored: bool,
}

impl ClaudeCollector {
    fn push(&mut self, event: claude::StreamEvent) {
        let claude::StreamEvent::Known(event) = event else {
            return;
        };

        match *event {
            claude::KnownStreamEvent::MessageStart { message, .. } => {
                self.push_message_start(*message);
            }
            claude::KnownStreamEvent::ContentBlockStart {
                index,
                content_block,
                ..
            } => {
                self.content
                    .insert(index, BlockState::from_block(*content_block));
            }
            claude::KnownStreamEvent::ContentBlockDelta { index, delta, .. } => {
                self.block_state(index).push_delta(*delta);
            }
            claude::KnownStreamEvent::ContentBlockStop { .. }
            | claude::KnownStreamEvent::MessageStop { .. }
            | claude::KnownStreamEvent::Ping { .. } => {}
            claude::KnownStreamEvent::MessageDelta {
                context_management,
                delta,
                usage,
                ..
            } => {
                let delta = *delta;
                self.container = delta.container.or(self.container.take());
                self.stop_reason = delta.stop_reason.or(self.stop_reason.take());
                self.stop_sequence = delta.stop_sequence.or(self.stop_sequence.take());
                self.stop_details = delta.stop_details.or(self.stop_details.take());
                self.context_management = context_management
                    .map(|value| *value)
                    .or(self.context_management.take());
                self.usage = usage.map(|value| *value).or(self.usage.take());
            }
            claude::KnownStreamEvent::Error { .. } => {
                self.errored = true;
            }
        }
    }

    fn push_message_start(&mut self, message: claude::CreateMessageStartBody) {
        self.id = Some(message.id);
        self.type_ = Some(message.type_);
        self.role = Some(message.role);
        self.model = Some(message.model);
        self.stop_reason = message.stop_reason.or(self.stop_reason.take());
        self.stop_sequence = message.stop_sequence.or(self.stop_sequence.take());
        self.usage = Some(message.usage);

        for (index, block) in message.content.into_iter().enumerate() {
            self.content.insert(
                u64::try_from(index).unwrap_or(u64::MAX),
                BlockState::from_block(block),
            );
        }
    }

    fn block_state(&mut self, index: u64) -> &mut BlockState {
        self.content.entry(index).or_insert_with(BlockState::text)
    }

    fn finish(self) -> claude::CreateMessageResponseBody {
        let stop_reason = if self.errored {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        } else {
            self.stop_reason
                .unwrap_or(claude::StopReason::Known(claude::StopReasonKnown::EndTurn))
        };

        claude::CreateMessageResponseBody {
            id: self.id.unwrap_or_default(),
            type_: self.type_.unwrap_or(claude::MessageObjectType::Known(
                claude::MessageObjectTypeKnown::Message,
            )),
            role: self.role.unwrap_or(claude::AssistantRole::Known(
                claude::AssistantRoleKnown::Assistant,
            )),
            content: self.content.into_values().map(BlockState::finish).collect(),
            model: self.model.unwrap_or_else(|| String::new().into()),
            stop_reason,
            stop_sequence: self.stop_sequence,
            usage: self.usage.unwrap_or_else(common::empty_claude_usage),
            container: self.container,
            context_management: self.context_management,
            diagnostics: None,
            stop_details: self.stop_details,
            extra: Default::default(),
        }
    }
}

struct BlockState {
    block: claude::ContentBlock,
    input_json: String,
}

impl BlockState {
    fn from_block(block: claude::ContentBlock) -> Self {
        Self {
            block,
            input_json: String::new(),
        }
    }

    fn text() -> Self {
        Self::from_block(claude::ContentBlock::Text(claude::ResponseTextBlock {
            citations: None,
            text: String::new(),
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        }))
    }

    fn thinking() -> Self {
        Self::from_block(claude::ContentBlock::Thinking(claude::ThinkingBlock {
            signature: String::new(),
            thinking: String::new(),
            type_: claude::ThinkingBlockType::Thinking,
        }))
    }

    fn compaction(content: String, encrypted_content: String) -> Self {
        Self::from_block(claude::ContentBlock::Compaction(
            claude::ResponseCompactionBlock {
                content: Some(content),
                encrypted_content,
                type_: claude::CompactionBlockType::Compaction,
                extra: Default::default(),
            },
        ))
    }

    fn push_delta(&mut self, delta: claude::EventDelta) {
        let claude::EventDelta::Known(delta) = delta else {
            return;
        };

        match *delta {
            claude::KnownEventDelta::Text { text, .. } => self.push_text(text),
            claude::KnownEventDelta::Thinking { thinking, .. } => self.push_thinking(thinking),
            claude::KnownEventDelta::Signature { signature, .. } => {
                self.set_thinking_signature(signature);
            }
            claude::KnownEventDelta::InputJson { partial_json, .. } => {
                self.input_json.push_str(&partial_json);
            }
            claude::KnownEventDelta::Citations { citation, .. } => {
                self.push_citation(*citation);
            }
            claude::KnownEventDelta::Compaction {
                content,
                encrypted_content,
                ..
            } => {
                self.push_compaction(content, encrypted_content);
            }
        }
    }

    fn push_text(&mut self, value: String) {
        match &mut self.block {
            claude::ContentBlock::Text(block) => block.text.push_str(&value),
            _ => {
                *self = Self::text();
                if let claude::ContentBlock::Text(block) = &mut self.block {
                    block.text.push_str(&value);
                }
            }
        }
    }

    fn push_thinking(&mut self, value: String) {
        match &mut self.block {
            claude::ContentBlock::Thinking(block) => block.thinking.push_str(&value),
            _ => {
                *self = Self::thinking();
                if let claude::ContentBlock::Thinking(block) = &mut self.block {
                    block.thinking.push_str(&value);
                }
            }
        }
    }

    fn set_thinking_signature(&mut self, value: String) {
        match &mut self.block {
            claude::ContentBlock::Thinking(block) => block.signature = value,
            _ => {
                *self = Self::thinking();
                if let claude::ContentBlock::Thinking(block) = &mut self.block {
                    block.signature = value;
                }
            }
        }
    }

    fn push_citation(&mut self, value: claude::Citation) {
        if let claude::ContentBlock::Text(block) = &mut self.block {
            block.citations.get_or_insert_with(Vec::new).push(value);
        }
    }

    fn push_compaction(&mut self, content: String, encrypted_content: String) {
        match &mut self.block {
            claude::ContentBlock::Compaction(block) => {
                let target = block.content.get_or_insert_with(String::new);
                target.push_str(&content);
                block.encrypted_content = encrypted_content;
            }
            _ => {
                *self = Self::compaction(content, encrypted_content);
            }
        }
    }

    fn finish(mut self) -> claude::ContentBlock {
        if !self.input_json.is_empty() {
            let input = parse_json_object(&self.input_json);
            match &mut self.block {
                claude::ContentBlock::ToolUse(block) => block.input = input,
                claude::ContentBlock::ServerToolUse(block) => block.input = input,
                claude::ContentBlock::McpToolUse(block) => block.input = input,
                _ => {}
            }
        }

        strip_block_extra(self.block)
    }
}

fn parse_json_object(value: &str) -> claude::JsonObject {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .map(|object| object.into_iter().collect())
        .unwrap_or_default()
}

fn strip_block_extra(block: claude::ContentBlock) -> claude::ContentBlock {
    match block {
        claude::ContentBlock::Text(block) => {
            claude::ContentBlock::Text(claude::ResponseTextBlock {
                citations: block.citations,
                text: block.text,
                type_: block.type_,
                extra: Default::default(),
            })
        }
        claude::ContentBlock::ToolUse(block) => {
            claude::ContentBlock::ToolUse(claude::ResponseToolUseBlock {
                id: block.id,
                input: block.input,
                name: block.name,
                type_: block.type_,
                caller: block.caller,
                extra: Default::default(),
            })
        }
        claude::ContentBlock::ServerToolUse(block) => {
            claude::ContentBlock::ServerToolUse(claude::ResponseServerToolUseBlock {
                id: block.id,
                input: block.input,
                name: block.name,
                type_: block.type_,
                caller: block.caller,
                extra: Default::default(),
            })
        }
        claude::ContentBlock::McpToolUse(block) => {
            claude::ContentBlock::McpToolUse(claude::ResponseMcpToolUseBlock {
                id: block.id,
                input: block.input,
                name: block.name,
                server_name: block.server_name,
                type_: block.type_,
                extra: Default::default(),
            })
        }
        claude::ContentBlock::Compaction(block) => {
            claude::ContentBlock::Compaction(claude::ResponseCompactionBlock {
                content: block.content,
                encrypted_content: block.encrypted_content,
                type_: block.type_,
                extra: Default::default(),
            })
        }
        block => block,
    }
}
