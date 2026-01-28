use std::collections::BTreeMap;

use gproxy_protocol::claude::create_message::stream::{
    BetaStreamContentBlock, BetaStreamContentBlockDelta, BetaStreamEvent, BetaStreamEventKnown,
    BetaStreamUsage,
};
use gproxy_protocol::claude::create_message::types::BetaStopReason;
use gproxy_protocol::claude::error::ErrorDetail;
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::stream::{
    ResponseCompletedEvent, ResponseCreatedEvent, ResponseErrorEvent,
    ResponseFunctionCallArgumentsDeltaEvent, ResponseFunctionCallArgumentsDoneEvent,
    ResponseInProgressEvent, ResponseMCPCallArgumentsDeltaEvent, ResponseMCPCallArgumentsDoneEvent,
    ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent, ResponseStreamEvent,
    ResponseTextDeltaEvent, ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, MCPToolCall, MCPToolCallStatus,
    MCPToolCallType, MessageStatus, OutputItem, OutputMessage, OutputMessageContent,
    OutputMessageRole, OutputMessageType, OutputTextContent, RefusalContent,
    ResponseIncompleteDetails, ResponseIncompleteReason, ResponseStatus, ResponseUsage,
    ResponseUsageInputTokensDetails, ResponseUsageOutputTokensDetails,
};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Mcp,
}

#[derive(Debug, Clone)]
struct ToolBlockInfo {
    output_index: i64,
    item_id: String,
    name: String,
    kind: ToolKind,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct ClaudeToOpenAIResponseStreamState {
    id: String,
    model: String,
    created_at: i64,
    sequence_number: i64,
    next_output_index: i64,
    message_added: bool,
    text_buffer: String,
    tool_blocks: BTreeMap<u32, ToolBlockInfo>,
    output_items: Vec<OutputItem>,
    stop_reason: Option<BetaStopReason>,
    usage: Option<BetaStreamUsage>,
}

impl ClaudeToOpenAIResponseStreamState {
    pub fn new(created_at: i64) -> Self {
        Self {
            id: "unknown".to_string(),
            model: "unknown".to_string(),
            created_at,
            sequence_number: 0,
            next_output_index: 0,
            message_added: false,
            text_buffer: String::new(),
            tool_blocks: BTreeMap::new(),
            output_items: Vec::new(),
            stop_reason: None,
            usage: None,
        }
    }

    pub fn transform_event(&mut self, event: BetaStreamEvent) -> Vec<ResponseStreamEvent> {
        let event = match event {
            BetaStreamEvent::Known(event) => event,
            BetaStreamEvent::Unknown(_) => return Vec::new(),
        };

        match event {
            BetaStreamEventKnown::MessageStart { message } => {
                self.id = message.id;
                self.model = map_model(&message.model);
                vec![ResponseStreamEvent::Created(ResponseCreatedEvent {
                    response: self.response_skeleton(ResponseStatus::InProgress, None, None, None),
                    sequence_number: self.next_sequence(),
                })]
            }
            BetaStreamEventKnown::ContentBlockStart {
                index,
                content_block,
            } => self.handle_block_start(index, content_block),
            BetaStreamEventKnown::ContentBlockDelta { index, delta } => {
                self.handle_block_delta(index, delta)
            }
            BetaStreamEventKnown::ContentBlockStop { index } => self.handle_block_stop(index),
            BetaStreamEventKnown::MessageDelta { delta, usage } => {
                self.stop_reason = delta.stop_reason;
                if usage.input_tokens.is_some() || usage.output_tokens.is_some() {
                    self.usage = Some(usage);
                }
                Vec::new()
            }
            BetaStreamEventKnown::MessageStop => self.finish_response(),
            BetaStreamEventKnown::Ping => {
                vec![ResponseStreamEvent::InProgress(ResponseInProgressEvent {
                    response: self.response_skeleton(ResponseStatus::InProgress, None, None, None),
                    sequence_number: self.next_sequence(),
                })]
            }
            BetaStreamEventKnown::Error { error, .. } => {
                vec![ResponseStreamEvent::Error(map_error(
                    error,
                    self.next_sequence(),
                ))]
            }
        }
    }

    fn handle_block_start(
        &mut self,
        index: u32,
        content_block: BetaStreamContentBlock,
    ) -> Vec<ResponseStreamEvent> {
        match content_block {
            BetaStreamContentBlock::Text(text) => self.emit_text(text.text),
            BetaStreamContentBlock::Thinking(thinking) => self.emit_text(thinking.thinking),
            BetaStreamContentBlock::RedactedThinking(thinking) => self.emit_text(thinking.data),
            BetaStreamContentBlock::ToolUse(tool) => {
                self.start_tool(index, tool.id, tool.name, ToolKind::Function)
            }
            BetaStreamContentBlock::ServerToolUse(tool) => self.start_tool(
                index,
                tool.id,
                format!("{:?}", tool.name),
                ToolKind::Function,
            ),
            BetaStreamContentBlock::McpToolUse(tool) => {
                self.start_tool(index, tool.id, tool.name, ToolKind::Mcp)
            }
            _ => Vec::new(),
        }
    }

    fn handle_block_delta(
        &mut self,
        index: u32,
        delta: BetaStreamContentBlockDelta,
    ) -> Vec<ResponseStreamEvent> {
        match delta {
            BetaStreamContentBlockDelta::TextDelta { text } => self.emit_text(text),
            BetaStreamContentBlockDelta::ThinkingDelta { thinking } => self.emit_text(thinking),
            BetaStreamContentBlockDelta::InputJsonDelta { partial_json } => {
                self.append_tool_arguments(index, partial_json)
            }
            BetaStreamContentBlockDelta::SignatureDelta { .. } => Vec::new(),
        }
    }

    fn handle_block_stop(&mut self, index: u32) -> Vec<ResponseStreamEvent> {
        let info = match self.tool_blocks.remove(&index) {
            Some(info) => info,
            None => return Vec::new(),
        };

        let mut events = Vec::new();
        match info.kind {
            ToolKind::Function => {
                events.push(ResponseStreamEvent::FunctionCallArgumentsDone(
                    ResponseFunctionCallArgumentsDoneEvent {
                        item_id: info.item_id.clone(),
                        name: info.name.clone(),
                        output_index: info.output_index,
                        arguments: info.arguments.clone(),
                        sequence_number: self.next_sequence(),
                    },
                ));

                let item = OutputItem::Function(FunctionToolCall {
                    r#type: FunctionToolCallType::FunctionCall,
                    id: Some(info.item_id.clone()),
                    call_id: info.item_id.clone(),
                    name: info.name.clone(),
                    arguments: info.arguments,
                    status: Some(FunctionCallItemStatus::Completed),
                });
                events.push(ResponseStreamEvent::OutputItemDone(
                    ResponseOutputItemDoneEvent {
                        output_index: info.output_index,
                        item: item.clone(),
                        sequence_number: self.next_sequence(),
                    },
                ));
                self.output_items.push(item);
            }
            ToolKind::Mcp => {
                events.push(ResponseStreamEvent::MCPCallArgumentsDone(
                    ResponseMCPCallArgumentsDoneEvent {
                        output_index: info.output_index,
                        item_id: info.item_id.clone(),
                        arguments: info.arguments.clone(),
                        sequence_number: self.next_sequence(),
                    },
                ));

                let item = OutputItem::MCPCall(MCPToolCall {
                    r#type: MCPToolCallType::MCPCall,
                    id: info.item_id.clone(),
                    server_label: "mcp".to_string(),
                    name: info.name.clone(),
                    arguments: info.arguments,
                    output: None,
                    error: None,
                    status: MCPToolCallStatus::Completed,
                    approval_request_id: None,
                });
                events.push(ResponseStreamEvent::OutputItemDone(
                    ResponseOutputItemDoneEvent {
                        output_index: info.output_index,
                        item: item.clone(),
                        sequence_number: self.next_sequence(),
                    },
                ));
                self.output_items.push(item);
            }
        }

        events
    }

    fn emit_text(&mut self, text: String) -> Vec<ResponseStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut events = Vec::new();
        if !self.message_added {
            self.message_added = true;
            let message = OutputItem::Message(OutputMessage {
                id: "message".to_string(),
                r#type: OutputMessageType::Message,
                role: OutputMessageRole::Assistant,
                content: Vec::new(),
                status: MessageStatus::InProgress,
            });
            events.push(ResponseStreamEvent::OutputItemAdded(
                ResponseOutputItemAddedEvent {
                    output_index: self.next_output_index,
                    item: message,
                    sequence_number: self.next_sequence(),
                },
            ));
            self.next_output_index += 1;
        }

        self.text_buffer.push_str(&text);
        events.push(ResponseStreamEvent::OutputTextDelta(
            ResponseTextDeltaEvent {
                item_id: "message".to_string(),
                output_index: 0,
                content_index: 0,
                delta: text,
                sequence_number: self.next_sequence(),
                logprobs: Vec::new(),
            },
        ));

        events
    }

    fn start_tool(
        &mut self,
        index: u32,
        id: String,
        name: String,
        kind: ToolKind,
    ) -> Vec<ResponseStreamEvent> {
        let output_index = self.next_output_index;
        self.next_output_index += 1;

        let item = match kind {
            ToolKind::Function => OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(id.clone()),
                call_id: id.clone(),
                name: name.clone(),
                arguments: String::new(),
                status: Some(FunctionCallItemStatus::InProgress),
            }),
            ToolKind::Mcp => OutputItem::MCPCall(MCPToolCall {
                r#type: MCPToolCallType::MCPCall,
                id: id.clone(),
                server_label: "mcp".to_string(),
                name: name.clone(),
                arguments: String::new(),
                output: None,
                error: None,
                status: MCPToolCallStatus::InProgress,
                approval_request_id: None,
            }),
        };

        let events = vec![ResponseStreamEvent::OutputItemAdded(
            ResponseOutputItemAddedEvent {
                output_index,
                item,
                sequence_number: self.next_sequence(),
            },
        )];

        self.tool_blocks.insert(
            index,
            ToolBlockInfo {
                output_index,
                item_id: id,
                name,
                kind,
                arguments: String::new(),
            },
        );

        events
    }

    fn append_tool_arguments(&mut self, index: u32, delta: String) -> Vec<ResponseStreamEvent> {
        let info = match self.tool_blocks.get_mut(&index) {
            Some(info) => info,
            None => return Vec::new(),
        };

        info.arguments.push_str(&delta);
        match info.kind {
            ToolKind::Function => vec![ResponseStreamEvent::FunctionCallArgumentsDelta(
                ResponseFunctionCallArgumentsDeltaEvent {
                    item_id: info.item_id.clone(),
                    output_index: info.output_index,
                    delta,
                    sequence_number: self.next_sequence(),
                },
            )],
            ToolKind::Mcp => vec![ResponseStreamEvent::MCPCallArgumentsDelta(
                ResponseMCPCallArgumentsDeltaEvent {
                    output_index: info.output_index,
                    item_id: info.item_id.clone(),
                    delta,
                    sequence_number: self.next_sequence(),
                },
            )],
        }
    }

    fn finish_response(&mut self) -> Vec<ResponseStreamEvent> {
        let mut events = Vec::new();

        if self.message_added {
            let content = if matches!(self.stop_reason, Some(BetaStopReason::Refusal)) {
                vec![OutputMessageContent::Refusal(RefusalContent {
                    refusal: self.text_buffer.clone(),
                })]
            } else {
                vec![OutputMessageContent::OutputText(OutputTextContent {
                    text: self.text_buffer.clone(),
                    annotations: Vec::new(),
                    logprobs: None,
                })]
            };

            events.push(ResponseStreamEvent::OutputTextDone(ResponseTextDoneEvent {
                item_id: "message".to_string(),
                output_index: 0,
                content_index: 0,
                text: self.text_buffer.clone(),
                sequence_number: self.next_sequence(),
                logprobs: Vec::new(),
            }));

            let status = if matches!(
                self.stop_reason,
                Some(BetaStopReason::MaxTokens | BetaStopReason::ModelContextWindowExceeded)
            ) {
                MessageStatus::Incomplete
            } else {
                MessageStatus::Completed
            };

            let message = OutputItem::Message(OutputMessage {
                id: "message".to_string(),
                r#type: OutputMessageType::Message,
                role: OutputMessageRole::Assistant,
                content,
                status,
            });

            events.push(ResponseStreamEvent::OutputItemDone(
                ResponseOutputItemDoneEvent {
                    output_index: 0,
                    item: message.clone(),
                    sequence_number: self.next_sequence(),
                },
            ));
            self.output_items.insert(0, message);
        }

        let (status, incomplete_details) = map_status(self.stop_reason);
        let usage = self.usage.as_ref().and_then(map_usage);

        events.push(ResponseStreamEvent::Completed(ResponseCompletedEvent {
            response: self.response_skeleton(
                status,
                usage,
                incomplete_details,
                Some(self.output_items.clone()),
            ),
            sequence_number: self.next_sequence(),
        }));

        events
    }

    fn response_skeleton(
        &self,
        status: ResponseStatus,
        usage: Option<ResponseUsage>,
        incomplete_details: Option<ResponseIncompleteDetails>,
        output: Option<Vec<OutputItem>>,
    ) -> Response {
        Response {
            id: self.id.clone(),
            object: ResponseObjectType::Response,
            created_at: self.created_at,
            status: Some(status),
            completed_at: None,
            error: None,
            incomplete_details,
            instructions: None,
            model: self.model.clone(),
            output: output.unwrap_or_default(),
            output_text: if self.text_buffer.is_empty() {
                None
            } else {
                Some(self.text_buffer.clone())
            },
            usage,
            parallel_tool_calls: None,
            conversation: None,
            previous_response_id: None,
            reasoning: None,
            background: None,
            max_output_tokens: None,
            max_tool_calls: None,
            text: None,
            tools: None,
            tool_choice: None,
            prompt: None,
            truncation: None,
            metadata: None,
            temperature: None,
            top_p: None,
            top_logprobs: None,
            user: None,
            safety_identifier: None,
            prompt_cache_key: None,
            service_tier: None,
            prompt_cache_retention: None,
            store: None,
        }
    }

    fn next_sequence(&mut self) -> i64 {
        let value = self.sequence_number;
        self.sequence_number += 1;
        value
    }
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

fn map_status(
    stop_reason: Option<BetaStopReason>,
) -> (ResponseStatus, Option<ResponseIncompleteDetails>) {
    match stop_reason {
        Some(BetaStopReason::MaxTokens) | Some(BetaStopReason::ModelContextWindowExceeded) => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::MaxOutputTokens,
            }),
        ),
        _ => (ResponseStatus::Completed, None),
    }
}

fn map_usage(usage: &BetaStreamUsage) -> Option<ResponseUsage> {
    let input_tokens = usage.input_tokens? as i64;
    let output_tokens = usage.output_tokens? as i64;
    Some(ResponseUsage {
        input_tokens,
        input_tokens_details: ResponseUsageInputTokensDetails { cached_tokens: 0 },
        output_tokens,
        output_tokens_details: ResponseUsageOutputTokensDetails {
            reasoning_tokens: 0,
        },
        total_tokens: input_tokens + output_tokens,
    })
}

fn map_error(error: ErrorDetail, sequence_number: i64) -> ResponseErrorEvent {
    ResponseErrorEvent {
        code: Some(format!("{:?}", error.r#type)),
        message: error.message,
        param: None,
        sequence_number,
    }
}
