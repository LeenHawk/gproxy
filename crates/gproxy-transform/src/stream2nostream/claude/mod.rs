use std::collections::BTreeMap;

use gproxy_protocol::claude::create_message::response::CreateMessageResponse;
use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamMessage, BetaStreamMessageDelta, BetaStreamUsage,
};
use gproxy_protocol::claude::create_message::types::{
    BetaCacheCreation, BetaContentBlock, BetaMessage, BetaServerToolUsage, BetaServiceTierUsed,
    BetaThinkingBlock, BetaToolCaller, BetaServerToolUseBlock, BetaStopReason, JsonObject,
};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
pub struct ClaudeStreamToMessageState {
    message: Option<BetaMessage>,
    stream_blocks: BTreeMap<u32, BetaStreamContentBlock>,
    content_blocks: BTreeMap<u32, BetaContentBlock>,
    pending_json: BTreeMap<u32, String>,
}

impl ClaudeStreamToMessageState {
    pub fn new() -> Self {
        Self {
            message: None,
            stream_blocks: BTreeMap::new(),
            content_blocks: BTreeMap::new(),
            pending_json: BTreeMap::new(),
        }
    }

    pub fn push_event(&mut self, event: BetaStreamEvent) -> Option<CreateMessageResponse> {
        match event {
            BetaStreamEvent::Known(known) => self.push_known_event(known),
            BetaStreamEvent::Unknown(_) => None,
        }
    }

    pub fn finalize(&mut self) -> Option<CreateMessageResponse> {
        let mut message = self.message.take()?;
        message.content = self.ordered_content();
        Some(message)
    }

    pub fn finalize_on_eof(&mut self) -> Option<CreateMessageResponse> {
        let mut message = self.message.take()?;
        if message.stop_reason.is_none() {
            message.stop_reason = Some(BetaStopReason::PauseTurn);
        }
        message.content = self.ordered_content();
        Some(message)
    }

    fn push_known_event(&mut self, event: BetaStreamEventKnown) -> Option<CreateMessageResponse> {
        match event {
            BetaStreamEventKnown::MessageStart { message } => {
                self.message = Some(map_message_start(message));
                None
            }
            BetaStreamEventKnown::ContentBlockStart { index, content_block } => {
                self.stream_blocks.insert(index, content_block);
                None
            }
            BetaStreamEventKnown::ContentBlockDelta { index, delta } => {
                self.handle_content_block_delta(index, delta);
                None
            }
            BetaStreamEventKnown::ContentBlockStop { index } => {
                self.finish_content_block(index);
                None
            }
            BetaStreamEventKnown::MessageDelta { delta, usage } => {
                self.handle_message_delta(delta, usage);
                None
            }
            BetaStreamEventKnown::MessageStop => self.finalize(),
            BetaStreamEventKnown::Ping => None,
            BetaStreamEventKnown::Error { .. } => None,
        }
    }

    fn handle_content_block_delta(&mut self, index: u32, delta: BetaStreamContentBlockDelta) {
        match delta {
            BetaStreamContentBlockDelta::TextDelta { text } => {
                if let Some(BetaStreamContentBlock::Text(block)) = self.stream_blocks.get_mut(&index)
                {
                    block.text.push_str(&text);
                }
            }
            BetaStreamContentBlockDelta::ThinkingDelta { thinking } => {
                if let Some(BetaStreamContentBlock::Thinking(block)) =
                    self.stream_blocks.get_mut(&index)
                {
                    block.thinking.push_str(&thinking);
                }
            }
            BetaStreamContentBlockDelta::SignatureDelta { signature } => {
                if let Some(BetaStreamContentBlock::Thinking(block)) =
                    self.stream_blocks.get_mut(&index)
                {
                    if let Some(existing) = &mut block.signature {
                        existing.push_str(&signature);
                    } else {
                        block.signature = Some(signature);
                    }
                }
            }
            BetaStreamContentBlockDelta::InputJsonDelta { partial_json } => {
                self.pending_json
                    .entry(index)
                    .and_modify(|value| value.push_str(&partial_json))
                    .or_insert(partial_json);
            }
        }
    }

    fn finish_content_block(&mut self, index: u32) {
        let mut block = match self.stream_blocks.remove(&index) {
            Some(block) => block,
            None => return,
        };

        if let Some(json) = self.pending_json.remove(&index)
            && let Ok(value) = serde_json::from_str::<JsonValue>(&json)
                && let Some(object) = value.as_object() {
                    let mapped = object
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<JsonObject>();
                    match &mut block {
                        BetaStreamContentBlock::ToolUse(tool) => tool.input = mapped,
                        BetaStreamContentBlock::ServerToolUse(tool) => tool.input = mapped,
                        BetaStreamContentBlock::McpToolUse(tool) => tool.input = mapped,
                        _ => {}
                    }
                }

        let content = map_stream_block(block);
        self.content_blocks.insert(index, content);
    }

    fn handle_message_delta(&mut self, delta: BetaStreamMessageDelta, usage: BetaStreamUsage) {
        if let Some(message) = self.message.as_mut() {
            if delta.stop_reason.is_some() {
                message.stop_reason = delta.stop_reason;
            }
            if delta.stop_sequence.is_some() {
                message.stop_sequence = delta.stop_sequence;
            }
            message.usage = map_usage(&usage);
        }
    }

    fn ordered_content(&self) -> Vec<BetaContentBlock> {
        self.content_blocks.values().cloned().collect()
    }
}

impl Default for ClaudeStreamToMessageState {
    fn default() -> Self {
        Self::new()
    }
}

fn map_message_start(message: BetaStreamMessage) -> BetaMessage {
    BetaMessage {
        id: message.id,
        container: message.container,
        content: message
            .content
            .into_iter()
            .map(map_stream_block)
            .collect(),
        context_management: message.context_management,
        model: message.model,
        role: message.role,
        stop_reason: message.stop_reason,
        stop_sequence: message.stop_sequence,
        r#type: message.r#type,
        usage: map_usage(&message.usage),
    }
}

fn map_stream_block(block: BetaStreamContentBlock) -> BetaContentBlock {
    match block {
        BetaStreamContentBlock::Text(text) => BetaContentBlock::Text(text),
        BetaStreamContentBlock::Thinking(block) => {
            BetaContentBlock::Thinking(BetaThinkingBlock {
                signature: block.signature.unwrap_or_default(),
                thinking: block.thinking,
                r#type: block.r#type,
            })
        }
        BetaStreamContentBlock::RedactedThinking(block) => BetaContentBlock::RedactedThinking(block),
        BetaStreamContentBlock::ToolUse(block) => BetaContentBlock::ToolUse(block),
        BetaStreamContentBlock::ServerToolUse(block) => {
            BetaContentBlock::ServerToolUse(BetaServerToolUseBlock {
                id: block.id,
                caller: block.caller.unwrap_or(BetaToolCaller::Direct),
                input: block.input,
                name: block.name,
                r#type: block.r#type,
            })
        }
        BetaStreamContentBlock::WebSearchToolResult(block) => {
            BetaContentBlock::WebSearchToolResult(block)
        }
        BetaStreamContentBlock::WebFetchToolResult(block) => {
            BetaContentBlock::WebFetchToolResult(block)
        }
        BetaStreamContentBlock::CodeExecutionToolResult(block) => {
            BetaContentBlock::CodeExecutionToolResult(block)
        }
        BetaStreamContentBlock::BashCodeExecutionToolResult(block) => {
            BetaContentBlock::BashCodeExecutionToolResult(block)
        }
        BetaStreamContentBlock::TextEditorCodeExecutionToolResult(block) => {
            BetaContentBlock::TextEditorCodeExecutionToolResult(block)
        }
        BetaStreamContentBlock::ToolSearchToolResult(block) => {
            BetaContentBlock::ToolSearchToolResult(block)
        }
        BetaStreamContentBlock::McpToolUse(block) => BetaContentBlock::McpToolUse(block),
        BetaStreamContentBlock::McpToolResult(block) => BetaContentBlock::McpToolResult(block),
        BetaStreamContentBlock::ContainerUpload(block) => BetaContentBlock::ContainerUpload(block),
    }
}

fn map_usage(usage: &BetaStreamUsage) -> gproxy_protocol::claude::create_message::types::BetaUsage {
    gproxy_protocol::claude::create_message::types::BetaUsage {
        cache_creation: usage
            .cache_creation
            .clone()
            .unwrap_or(BetaCacheCreation {
                ephemeral_1h_input_tokens: 0,
                ephemeral_5m_input_tokens: 0,
            }),
        cache_creation_input_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
        cache_read_input_tokens: usage.cache_read_input_tokens.unwrap_or(0),
        input_tokens: usage.input_tokens.unwrap_or(0),
        output_tokens: usage.output_tokens.unwrap_or(0),
        server_tool_use: Some(usage.server_tool_use.clone().unwrap_or(BetaServerToolUsage {
            web_fetch_requests: 0,
            web_search_requests: 0,
        })),
        // Stream usage doesn't include service tier; default to standard.
        service_tier: BetaServiceTierUsed::Standard,
    }
}
