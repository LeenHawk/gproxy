use std::collections::BTreeMap;

use gproxy_protocol::claude::count_tokens::types::Model as ClaudeModel;
use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamMessage, BetaStreamMessageDelta, BetaStreamUsage,
};
use gproxy_protocol::claude::create_message::types::{
    BetaMessageRole, BetaMessageType, BetaStopReason, BetaTextBlock, BetaTextBlockType,
    BetaToolUseBlock, BetaToolUseBlockType, JsonObject,
};
use gproxy_protocol::openai::create_chat_completions::stream::CreateChatCompletionStreamResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionFunctionCallDelta,
    ChatCompletionMessageToolCallChunk, CompletionUsage,
};

#[derive(Debug, Clone)]
struct ToolBlockInfo {
    block_index: u32,
}

#[derive(Debug, Clone)]
pub struct OpenAIToClaudeChatCompletionStreamState {
    id: String,
    model: ClaudeModel,
    message_started: bool,
    finish_emitted: bool,
    pending_finish: Option<BetaStopReason>,
    next_block_index: u32,
    text_block_index: Option<u32>,
    tool_blocks: BTreeMap<i64, ToolBlockInfo>,
}

impl OpenAIToClaudeChatCompletionStreamState {
    pub fn new() -> Self {
        Self {
            id: "unknown".to_string(),
            model: ClaudeModel::Custom("unknown".to_string()),
            message_started: false,
            finish_emitted: false,
            pending_finish: None,
            next_block_index: 0,
            text_block_index: None,
            tool_blocks: BTreeMap::new(),
        }
    }

    pub fn transform_chunk(
        &mut self,
        chunk: CreateChatCompletionStreamResponse,
    ) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();

        if !self.message_started {
            self.id = chunk.id.clone();
            self.model = ClaudeModel::Custom(chunk.model.clone());
            self.message_started = true;
            events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageStart {
                message: BetaStreamMessage {
                    id: self.id.clone(),
                    container: None,
                    content: Vec::new(),
                    context_management: None,
                    model: self.model.clone(),
                    role: BetaMessageRole::Assistant,
                    stop_reason: None,
                    stop_sequence: None,
                    r#type: BetaMessageType::Message,
                    usage: BetaStreamUsage {
                        input_tokens: None,
                        output_tokens: None,
                        cache_creation_input_tokens: None,
                        cache_read_input_tokens: None,
                        cache_creation: None,
                        server_tool_use: None,
                    },
                },
            }));
        }

        let choice = chunk.choices.first();

        if let Some(choice) = choice {
            if let Some(content) = &choice.delta.content {
                events.extend(self.emit_text(content));
            } else if let Some(reasoning) = &choice.delta.reasoning_content {
                events.extend(self.emit_text(reasoning));
            }

            if let Some(refusal) = &choice.delta.refusal {
                events.extend(self.emit_text(refusal));
            }

            if let Some(tool_calls) = &choice.delta.tool_calls {
                for call in tool_calls {
                    events.extend(self.emit_tool_call(call));
                }
            }

            if let Some(function_call) = &choice.delta.function_call {
                events.extend(self.emit_function_call(function_call));
            }
        }

        let usage = map_usage(chunk.usage);
        let finish_reason = choice.and_then(|choice| choice.finish_reason.map(map_finish_reason));

        if let Some(reason) = finish_reason
            && !self.finish_emitted {
                events.extend(self.close_open_blocks());
                self.pending_finish = Some(reason);
            }

        if let Some(usage) = usage {
            if let Some(reason) = self.pending_finish.take() {
                events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageDelta {
                    delta: BetaStreamMessageDelta {
                        stop_reason: Some(reason),
                        stop_sequence: None,
                    },
                    usage,
                }));
                events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageStop));
                self.finish_emitted = true;
            } else {
                events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageDelta {
                    delta: BetaStreamMessageDelta {
                        stop_reason: None,
                        stop_sequence: None,
                    },
                    usage,
                }));
            }
        }

        events
    }

    fn emit_text(&mut self, text: &str) -> Vec<BetaStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();
        let block_index = match self.text_block_index {
            Some(index) => index,
            None => {
                let index = self.next_block_index;
                self.next_block_index += 1;
                self.text_block_index = Some(index);
                events.push(BetaStreamEvent::Known(
                    BetaStreamEventKnown::ContentBlockStart {
                        index,
                        content_block: BetaStreamContentBlock::Text(BetaTextBlock {
                            citations: None,
                            text: String::new(),
                            r#type: BetaTextBlockType::Text,
                        }),
                    },
                ));
                index
            }
        };

        events.push(BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockDelta {
                index: block_index,
                delta: BetaStreamContentBlockDelta::TextDelta {
                    text: text.to_string(),
                },
            },
        ));

        events
    }

    fn emit_tool_call(
        &mut self,
        call: &ChatCompletionMessageToolCallChunk,
    ) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();
        let index = call.index;

        let info = self.tool_blocks.entry(index).or_insert_with(|| {
            let block_index = self.next_block_index;
            self.next_block_index += 1;
            let id = call
                .id
                .clone()
                .unwrap_or_else(|| format!("toolcall-{}", index));
            let name = call
                .function
                .as_ref()
                .and_then(|function| function.name.clone())
                .unwrap_or_else(|| "tool".to_string());

            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStart {
                    index: block_index,
                    content_block: BetaStreamContentBlock::ToolUse(BetaToolUseBlock {
                        id: id.clone(),
                        input: JsonObject::new(),
                        name: name.clone(),
                        r#type: BetaToolUseBlockType::ToolUse,
                        caller: None,
                    }),
                },
            ));

            ToolBlockInfo { block_index }
        });

        if let Some(function) = &call.function
            && let Some(arguments) = &function.arguments
        {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockDelta {
                    index: info.block_index,
                    delta: BetaStreamContentBlockDelta::InputJsonDelta {
                        partial_json: arguments.clone(),
                    },
                },
            ));
        }

        events
    }

    fn emit_function_call(
        &mut self,
        call: &ChatCompletionFunctionCallDelta,
    ) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();
        let key = -1;
        let info = self.tool_blocks.entry(key).or_insert_with(|| {
            let block_index = self.next_block_index;
            self.next_block_index += 1;
            let name = call
                .name
                .clone()
                .unwrap_or_else(|| "function_call".to_string());
            let id = "function_call".to_string();

            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStart {
                    index: block_index,
                    content_block: BetaStreamContentBlock::ToolUse(BetaToolUseBlock {
                        id: id.clone(),
                        input: JsonObject::new(),
                        name: name.clone(),
                        r#type: BetaToolUseBlockType::ToolUse,
                        caller: None,
                    }),
                },
            ));

            ToolBlockInfo { block_index }
        });

        if let Some(arguments) = &call.arguments {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockDelta {
                    index: info.block_index,
                    delta: BetaStreamContentBlockDelta::InputJsonDelta {
                        partial_json: arguments.clone(),
                    },
                },
            ));
        }

        events
    }

    fn close_open_blocks(&mut self) -> Vec<BetaStreamEvent> {
        let mut events = Vec::new();

        if let Some(index) = self.text_block_index.take() {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop { index },
            ));
        }

        let tool_blocks = std::mem::take(&mut self.tool_blocks);
        for (_, info) in tool_blocks {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop {
                    index: info.block_index,
                },
            ));
        }

        events
    }
}

impl Default for OpenAIToClaudeChatCompletionStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn map_finish_reason(reason: ChatCompletionFinishReason) -> BetaStopReason {
    match reason {
        ChatCompletionFinishReason::Stop => BetaStopReason::EndTurn,
        ChatCompletionFinishReason::Length => BetaStopReason::MaxTokens,
        ChatCompletionFinishReason::ToolCalls | ChatCompletionFinishReason::FunctionCall => {
            BetaStopReason::ToolUse
        }
        ChatCompletionFinishReason::ContentFilter => BetaStopReason::Refusal,
    }
}

fn map_usage(usage: Option<CompletionUsage>) -> Option<BetaStreamUsage> {
    let usage = usage?;
    Some(BetaStreamUsage {
        input_tokens: Some(usage.prompt_tokens.max(0) as u32),
        output_tokens: Some(usage.completion_tokens.max(0) as u32),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        server_tool_use: None,
    })
}
