use std::collections::BTreeMap;

use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamUsage,
};
use gproxy_protocol::claude::create_message::types::BetaStopReason;
use gproxy_protocol::claude::error::ErrorDetail;
use gproxy_protocol::openai::create_chat_completions::stream::{
    ChatCompletionChunkObjectType, ChatCompletionStreamChoice, CreateChatCompletionStreamResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCallChunk,
    ChatCompletionMessageToolCallChunkFunction, ChatCompletionRole,
    ChatCompletionStreamResponseDelta, ChatCompletionToolCallChunkType, CompletionUsage,
};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
struct ToolCallInfo {
    id: String,
    name: String,
}

#[derive(Debug, Clone)]
pub struct ClaudeToOpenAIChatCompletionStreamState {
    id: String,
    model: String,
    created: i64,
    tool_calls: BTreeMap<u32, ToolCallInfo>,
    role_emitted: bool,
    finish_emitted: bool,
}

impl ClaudeToOpenAIChatCompletionStreamState {
    pub fn new(created: i64) -> Self {
        Self {
            id: "unknown".to_string(),
            model: "unknown".to_string(),
            created,
            tool_calls: BTreeMap::new(),
            role_emitted: false,
            finish_emitted: false,
        }
    }

    pub fn transform_event(
        &mut self,
        event: BetaStreamEvent,
    ) -> Option<CreateChatCompletionStreamResponse> {
        match self.transform_event_with_control(event) {
            Some(ClaudeToOpenAIChatCompletionStreamEvent::Chunk(chunk)) => Some(chunk),
            _ => None,
        }
    }

    pub fn transform_event_with_control(
        &mut self,
        event: BetaStreamEvent,
    ) -> Option<ClaudeToOpenAIChatCompletionStreamEvent> {
        let event = match event {
            BetaStreamEvent::Known(event) => event,
            BetaStreamEvent::Unknown(_) => return None,
        };

        match event {
            BetaStreamEventKnown::MessageStart { message } => {
                self.id = message.id;
                self.model = map_model(&message.model);
                self.role_emitted = true;
                Some(ClaudeToOpenAIChatCompletionStreamEvent::Chunk(self.chunk(
                    ChatCompletionStreamResponseDelta {
                        role: Some(ChatCompletionRole::Assistant),
                        content: None,
                        reasoning_content: None,
                        function_call: None,
                        tool_calls: None,
                        refusal: None,
                        obfuscation: None,
                    },
                    None,
                    None,
                )))
            }
            BetaStreamEventKnown::ContentBlockStart {
                index,
                content_block,
            } => self
                .map_block_start(index, content_block)
                .map(ClaudeToOpenAIChatCompletionStreamEvent::Chunk),
            BetaStreamEventKnown::ContentBlockDelta { index, delta } => self
                .map_block_delta(index, delta)
                .map(ClaudeToOpenAIChatCompletionStreamEvent::Chunk),
            BetaStreamEventKnown::MessageDelta { delta, usage } => {
                let finish_reason = delta.stop_reason.map(map_finish_reason);
                if finish_reason.is_some() {
                    self.finish_emitted = true;
                }
                let usage = map_usage(&usage);
                if finish_reason.is_none() && usage.is_none() {
                    None
                } else {
                    Some(ClaudeToOpenAIChatCompletionStreamEvent::Chunk(self.chunk(
                        ChatCompletionStreamResponseDelta {
                            role: None,
                            content: None,
                            reasoning_content: None,
                            function_call: None,
                            tool_calls: None,
                            refusal: None,
                            obfuscation: None,
                        },
                        finish_reason,
                        usage,
                    )))
                }
            }
            BetaStreamEventKnown::MessageStop => {
                if !self.finish_emitted {
                    self.finish_emitted = true;
                    Some(ClaudeToOpenAIChatCompletionStreamEvent::Chunk(self.chunk(
                        ChatCompletionStreamResponseDelta {
                            role: None,
                            content: None,
                            reasoning_content: None,
                            function_call: None,
                            tool_calls: None,
                            refusal: None,
                            obfuscation: None,
                        },
                        Some(ChatCompletionFinishReason::Stop),
                        None,
                    )))
                } else {
                    Some(ClaudeToOpenAIChatCompletionStreamEvent::Done)
                }
            }
            BetaStreamEventKnown::Ping => Some(ClaudeToOpenAIChatCompletionStreamEvent::Ping),
            BetaStreamEventKnown::Error { error, .. } => {
                Some(ClaudeToOpenAIChatCompletionStreamEvent::Error(error))
            }
            BetaStreamEventKnown::ContentBlockStop { .. } => None,
        }
    }

    fn map_block_start(
        &mut self,
        index: u32,
        content_block: BetaStreamContentBlock,
    ) -> Option<CreateChatCompletionStreamResponse> {
        match content_block {
            BetaStreamContentBlock::Text(text) => {
                if text.text.is_empty() {
                    None
                } else {
                    Some(self.text_chunk(text.text))
                }
            }
            BetaStreamContentBlock::Thinking(thinking) => {
                if thinking.thinking.is_empty() {
                    None
                } else {
                    Some(self.text_chunk(thinking.thinking))
                }
            }
            BetaStreamContentBlock::RedactedThinking(thinking) => {
                if thinking.data.is_empty() {
                    None
                } else {
                    Some(self.text_chunk(thinking.data))
                }
            }
            BetaStreamContentBlock::ToolUse(tool) => {
                self.store_tool(index, tool.id, tool.name);
                Some(self.tool_call_start(index))
            }
            BetaStreamContentBlock::ServerToolUse(tool) => {
                self.store_tool(index, tool.id, format!("{:?}", tool.name));
                Some(self.tool_call_start(index))
            }
            BetaStreamContentBlock::McpToolUse(tool) => {
                self.store_tool(index, tool.id, tool.name);
                Some(self.tool_call_start(index))
            }
            _ => None,
        }
    }

    fn map_block_delta(
        &mut self,
        index: u32,
        delta: BetaStreamContentBlockDelta,
    ) -> Option<CreateChatCompletionStreamResponse> {
        match delta {
            BetaStreamContentBlockDelta::TextDelta { text } => {
                if text.is_empty() {
                    None
                } else {
                    Some(self.text_chunk(text))
                }
            }
            BetaStreamContentBlockDelta::ThinkingDelta { thinking } => {
                if thinking.is_empty() {
                    None
                } else {
                    Some(self.text_chunk(thinking))
                }
            }
            BetaStreamContentBlockDelta::InputJsonDelta { partial_json } => {
                if partial_json.is_empty() {
                    None
                } else {
                    Some(self.tool_call_delta(index, partial_json))
                }
            }
            BetaStreamContentBlockDelta::SignatureDelta { .. } => None,
        }
    }

    fn store_tool(&mut self, index: u32, id: String, name: String) {
        self.tool_calls.insert(index, ToolCallInfo { id, name });
    }

    fn tool_call_start(&self, index: u32) -> CreateChatCompletionStreamResponse {
        let info = self.tool_calls.get(&index);
        let tool_call = ChatCompletionMessageToolCallChunk {
            index: index as i64,
            id: info.map(|tool| tool.id.clone()),
            r#type: Some(ChatCompletionToolCallChunkType::Function),
            function: Some(ChatCompletionMessageToolCallChunkFunction {
                name: info.map(|tool| tool.name.clone()),
                arguments: None,
            }),
        };

        self.chunk(
            ChatCompletionStreamResponseDelta {
                role: None,
                content: None,
                reasoning_content: None,
                function_call: None,
                tool_calls: Some(vec![tool_call]),
                refusal: None,
                obfuscation: None,
            },
            None,
            None,
        )
    }

    fn tool_call_delta(
        &self,
        index: u32,
        partial_json: String,
    ) -> CreateChatCompletionStreamResponse {
        let tool_call = ChatCompletionMessageToolCallChunk {
            index: index as i64,
            id: None,
            r#type: Some(ChatCompletionToolCallChunkType::Function),
            function: Some(ChatCompletionMessageToolCallChunkFunction {
                name: None,
                arguments: Some(partial_json),
            }),
        };

        self.chunk(
            ChatCompletionStreamResponseDelta {
                role: None,
                content: None,
                reasoning_content: None,
                function_call: None,
                tool_calls: Some(vec![tool_call]),
                refusal: None,
                obfuscation: None,
            },
            None,
            None,
        )
    }

    fn text_chunk(&self, text: String) -> CreateChatCompletionStreamResponse {
        self.chunk(
            ChatCompletionStreamResponseDelta {
                role: None,
                content: Some(text),
                reasoning_content: None,
                function_call: None,
                tool_calls: None,
                refusal: None,
                obfuscation: None,
            },
            None,
            None,
        )
    }

    fn chunk(
        &self,
        delta: ChatCompletionStreamResponseDelta,
        finish_reason: Option<ChatCompletionFinishReason>,
        usage: Option<CompletionUsage>,
    ) -> CreateChatCompletionStreamResponse {
        CreateChatCompletionStreamResponse {
            id: self.id.clone(),
            object: ChatCompletionChunkObjectType::ChatCompletionChunk,
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChatCompletionStreamChoice {
                index: 0,
                delta,
                logprobs: None,
                finish_reason,
            }],
            usage,
            service_tier: None,
            system_fingerprint: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClaudeToOpenAIChatCompletionStreamEvent {
    Chunk(CreateChatCompletionStreamResponse),
    Done,
    Ping,
    Error(ErrorDetail),
}

fn map_finish_reason(reason: BetaStopReason) -> ChatCompletionFinishReason {
    match reason {
        BetaStopReason::MaxTokens | BetaStopReason::ModelContextWindowExceeded => {
            ChatCompletionFinishReason::Length
        }
        BetaStopReason::ToolUse => ChatCompletionFinishReason::ToolCalls,
        BetaStopReason::Refusal => ChatCompletionFinishReason::ContentFilter,
        BetaStopReason::StopSequence | BetaStopReason::EndTurn => ChatCompletionFinishReason::Stop,
        BetaStopReason::PauseTurn => ChatCompletionFinishReason::Stop,
    }
}

fn map_usage(usage: &BetaStreamUsage) -> Option<CompletionUsage> {
    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    if usage.input_tokens.is_none() && usage.output_tokens.is_none() {
        return None;
    }

    Some(CompletionUsage {
        prompt_tokens: input_tokens as i64,
        completion_tokens: output_tokens as i64,
        total_tokens: (input_tokens + output_tokens) as i64,
        completion_tokens_details: None,
        prompt_tokens_details: None,
    })
}

fn map_model(model: &gproxy_protocol::claude::count_tokens::types::Model) -> String {
    match model {
        gproxy_protocol::claude::count_tokens::types::Model::Custom(value) => value.clone(),
        gproxy_protocol::claude::count_tokens::types::Model::Known(known) => {
            match serde_json::to_value(known) {
                Ok(JsonValue::String(value)) => value,
                _ => "unknown".to_string(),
            }
        }
    }
}
