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
use gproxy_protocol::claude::error::{ErrorDetail, ErrorType};
use gproxy_protocol::openai::create_response::response::Response;
use gproxy_protocol::openai::create_response::stream::{
    ResponseCompletedEvent, ResponseCreatedEvent, ResponseErrorEvent,
    ResponseFunctionCallArgumentsDeltaEvent, ResponseFunctionCallArgumentsDoneEvent,
    ResponseInProgressEvent, ResponseMCPCallArgumentsDeltaEvent, ResponseMCPCallArgumentsDoneEvent,
    ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent, ResponseRefusalDeltaEvent,
    ResponseRefusalDoneEvent, ResponseStreamEvent, ResponseTextDeltaEvent, ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    OutputItem, ResponseIncompleteDetails, ResponseIncompleteReason, ResponseStatus, ResponseUsage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Mcp,
}

#[derive(Debug, Clone)]
struct ToolInfo {
    block_index: u32,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct OpenAIResponseToClaudeStreamState {
    id: String,
    model: ClaudeModel,
    message_started: bool,
    next_block_index: u32,
    text_block_index: Option<u32>,
    tool_blocks: BTreeMap<i64, ToolInfo>,
    stop_reason: Option<BetaStopReason>,
    usage: Option<ResponseUsage>,
    saw_refusal: bool,
}

impl OpenAIResponseToClaudeStreamState {
    pub fn new() -> Self {
        Self {
            id: "unknown".to_string(),
            model: ClaudeModel::Custom("unknown".to_string()),
            message_started: false,
            next_block_index: 0,
            text_block_index: None,
            tool_blocks: BTreeMap::new(),
            stop_reason: None,
            usage: None,
            saw_refusal: false,
        }
    }

    pub fn transform_event(&mut self, event: ResponseStreamEvent) -> Vec<BetaStreamEvent> {
        match event {
            ResponseStreamEvent::Created(event) => self.handle_created(event),
            ResponseStreamEvent::InProgress(event) => self.handle_in_progress(event),
            ResponseStreamEvent::Completed(event) => self.handle_completed(event),
            ResponseStreamEvent::Failed(event) => self.handle_completed(ResponseCompletedEvent {
                response: event.response,
                sequence_number: event.sequence_number,
            }),
            ResponseStreamEvent::Incomplete(event) => {
                self.handle_completed(ResponseCompletedEvent {
                    response: event.response,
                    sequence_number: event.sequence_number,
                })
            }
            ResponseStreamEvent::OutputItemAdded(event) => self.handle_output_item_added(event),
            ResponseStreamEvent::OutputItemDone(event) => self.handle_output_item_done(event),
            ResponseStreamEvent::OutputTextDelta(event) => self.handle_text_delta(event),
            ResponseStreamEvent::OutputTextDone(event) => self.handle_text_done(event),
            ResponseStreamEvent::RefusalDelta(event) => self.handle_refusal_delta(event),
            ResponseStreamEvent::RefusalDone(event) => self.handle_refusal_done(event),
            ResponseStreamEvent::FunctionCallArgumentsDelta(event) => {
                self.handle_function_call_delta(event)
            }
            ResponseStreamEvent::FunctionCallArgumentsDone(event) => {
                self.handle_function_call_done(event)
            }
            ResponseStreamEvent::MCPCallArgumentsDelta(event) => self.handle_mcp_call_delta(event),
            ResponseStreamEvent::MCPCallArgumentsDone(event) => self.handle_mcp_call_done(event),
            ResponseStreamEvent::Error(event) => {
                vec![BetaStreamEvent::Known(BetaStreamEventKnown::Error {
                    error: map_error(event),
                    request_id: None,
                })]
            }
            _ => Vec::new(),
        }
    }

    fn handle_created(&mut self, event: ResponseCreatedEvent) -> Vec<BetaStreamEvent> {
        self.update_from_response(&event.response);
        self.ensure_message_start()
    }

    fn handle_in_progress(&mut self, event: ResponseInProgressEvent) -> Vec<BetaStreamEvent> {
        self.update_from_response(&event.response);
        self.ensure_message_start()
    }

    fn handle_completed(&mut self, event: ResponseCompletedEvent) -> Vec<BetaStreamEvent> {
        self.update_from_response(&event.response);

        let mut events = self.ensure_message_start();
        events.extend(self.close_open_blocks());

        let stop_reason = self.stop_reason.or(if self.saw_refusal {
            Some(BetaStopReason::Refusal)
        } else {
            None
        });

        let usage = self.usage.as_ref().and_then(map_usage);

        events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageDelta {
            delta: BetaStreamMessageDelta {
                stop_reason,
                stop_sequence: None,
            },
            usage: usage.unwrap_or_else(empty_usage),
        }));
        events.push(BetaStreamEvent::Known(BetaStreamEventKnown::MessageStop));
        events
    }

    fn handle_output_item_added(
        &mut self,
        event: ResponseOutputItemAddedEvent,
    ) -> Vec<BetaStreamEvent> {
        let mut events = self.ensure_message_start();

        match event.item {
            OutputItem::Function(function) => {
                events.extend(self.start_tool(
                    event.output_index,
                    function.call_id.clone(),
                    function.name.clone(),
                    ToolKind::Function,
                    function.arguments.clone(),
                ));
            }
            OutputItem::MCPCall(mcp) => {
                events.extend(self.start_tool(
                    event.output_index,
                    mcp.id.clone(),
                    mcp.name.clone(),
                    ToolKind::Mcp,
                    mcp.arguments.clone(),
                ));
            }
            OutputItem::Message(_) => {}
            _ => {}
        }

        events
    }

    fn handle_output_item_done(
        &mut self,
        event: ResponseOutputItemDoneEvent,
    ) -> Vec<BetaStreamEvent> {
        if let Some(info) = self.tool_blocks.remove(&event.output_index) {
            vec![BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop {
                    index: info.block_index,
                },
            )]
        } else {
            Vec::new()
        }
    }

    fn handle_text_delta(&mut self, event: ResponseTextDeltaEvent) -> Vec<BetaStreamEvent> {
        self.emit_text(event.delta)
    }

    fn handle_text_done(&mut self, _event: ResponseTextDoneEvent) -> Vec<BetaStreamEvent> {
        if let Some(index) = self.text_block_index.take() {
            vec![BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockStop { index },
            )]
        } else {
            Vec::new()
        }
    }

    fn handle_refusal_delta(&mut self, event: ResponseRefusalDeltaEvent) -> Vec<BetaStreamEvent> {
        self.saw_refusal = true;
        self.emit_text(event.delta)
    }

    fn handle_refusal_done(&mut self, event: ResponseRefusalDoneEvent) -> Vec<BetaStreamEvent> {
        self.saw_refusal = true;
        self.emit_text(event.refusal)
    }

    fn handle_function_call_delta(
        &mut self,
        event: ResponseFunctionCallArgumentsDeltaEvent,
    ) -> Vec<BetaStreamEvent> {
        self.append_tool_arguments(
            event.output_index,
            event.item_id,
            event.delta,
            ToolKind::Function,
        )
    }

    fn handle_function_call_done(
        &mut self,
        event: ResponseFunctionCallArgumentsDoneEvent,
    ) -> Vec<BetaStreamEvent> {
        let item_id = event.item_id.clone();
        self.ensure_tool(
            event.output_index,
            item_id.clone(),
            event.name,
            ToolKind::Function,
        );
        self.apply_tool_arguments_done(
            event.output_index,
            item_id,
            event.arguments,
            ToolKind::Function,
        )
    }

    fn handle_mcp_call_delta(
        &mut self,
        event: ResponseMCPCallArgumentsDeltaEvent,
    ) -> Vec<BetaStreamEvent> {
        self.append_tool_arguments(
            event.output_index,
            event.item_id,
            event.delta,
            ToolKind::Mcp,
        )
    }

    fn handle_mcp_call_done(
        &mut self,
        event: ResponseMCPCallArgumentsDoneEvent,
    ) -> Vec<BetaStreamEvent> {
        self.apply_tool_arguments_done(
            event.output_index,
            event.item_id,
            event.arguments,
            ToolKind::Mcp,
        )
    }

    fn emit_text(&mut self, text: String) -> Vec<BetaStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut events = self.ensure_message_start();
        let index = match self.text_block_index {
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
                index,
                delta: BetaStreamContentBlockDelta::TextDelta { text },
            },
        ));
        events
    }

    fn start_tool(
        &mut self,
        output_index: i64,
        id: String,
        name: String,
        _kind: ToolKind,
        arguments: String,
    ) -> Vec<BetaStreamEvent> {
        let mut events = self.ensure_message_start();
        let block_index = self.next_block_index;
        self.next_block_index += 1;

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

        if !arguments.is_empty() {
            events.push(BetaStreamEvent::Known(
                BetaStreamEventKnown::ContentBlockDelta {
                    index: block_index,
                    delta: BetaStreamContentBlockDelta::InputJsonDelta {
                        partial_json: arguments.clone(),
                    },
                },
            ));
        }

        self.tool_blocks.insert(
            output_index,
            ToolInfo {
                block_index,
                arguments,
            },
        );

        events
    }

    fn ensure_tool(&mut self, output_index: i64, _id: String, _name: String, _kind: ToolKind) {
        if self.tool_blocks.contains_key(&output_index) {
            return;
        }
        let block_index = self.next_block_index;
        self.next_block_index += 1;
        self.tool_blocks.insert(
            output_index,
            ToolInfo {
                block_index,
                arguments: String::new(),
            },
        );
    }

    fn append_tool_arguments(
        &mut self,
        output_index: i64,
        id: String,
        delta: String,
        kind: ToolKind,
    ) -> Vec<BetaStreamEvent> {
        let info = if let Some(info) = self.tool_blocks.get_mut(&output_index) {
            info
        } else {
            self.ensure_tool(output_index, id, "tool".to_string(), kind);
            match self.tool_blocks.get_mut(&output_index) {
                Some(info) => info,
                None => return Vec::new(),
            }
        };

        info.arguments.push_str(&delta);
        vec![BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockDelta {
                index: info.block_index,
                delta: BetaStreamContentBlockDelta::InputJsonDelta {
                    partial_json: delta,
                },
            },
        )]
    }

    fn apply_tool_arguments_done(
        &mut self,
        output_index: i64,
        id: String,
        arguments: String,
        kind: ToolKind,
    ) -> Vec<BetaStreamEvent> {
        let info = if let Some(info) = self.tool_blocks.get_mut(&output_index) {
            info
        } else {
            self.ensure_tool(output_index, id, "tool".to_string(), kind);
            match self.tool_blocks.get_mut(&output_index) {
                Some(info) => info,
                None => return Vec::new(),
            }
        };

        let delta = if arguments.starts_with(&info.arguments) {
            arguments[info.arguments.len()..].to_string()
        } else {
            arguments.clone()
        };

        if delta.is_empty() {
            return Vec::new();
        }

        info.arguments = arguments;
        vec![BetaStreamEvent::Known(
            BetaStreamEventKnown::ContentBlockDelta {
                index: info.block_index,
                delta: BetaStreamContentBlockDelta::InputJsonDelta {
                    partial_json: delta,
                },
            },
        )]
    }

    fn ensure_message_start(&mut self) -> Vec<BetaStreamEvent> {
        if self.message_started {
            return Vec::new();
        }
        self.message_started = true;
        vec![BetaStreamEvent::Known(BetaStreamEventKnown::MessageStart {
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
                usage: empty_usage(),
            },
        })]
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

    fn update_from_response(&mut self, response: &Response) {
        self.id = response.id.clone();
        self.model = ClaudeModel::Custom(response.model.clone());
        if let Some(status) = response.status {
            self.stop_reason = map_status(status, response.incomplete_details.as_ref());
        }
        if let Some(usage) = &response.usage {
            self.usage = Some(usage.clone());
        }
    }
}

impl Default for OpenAIResponseToClaudeStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn empty_usage() -> BetaStreamUsage {
    BetaStreamUsage {
        input_tokens: None,
        output_tokens: None,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        server_tool_use: None,
    }
}

fn map_status(
    status: ResponseStatus,
    details: Option<&ResponseIncompleteDetails>,
) -> Option<BetaStopReason> {
    match status {
        ResponseStatus::Completed => Some(BetaStopReason::EndTurn),
        ResponseStatus::Incomplete => match details.map(|d| d.reason) {
            Some(ResponseIncompleteReason::MaxOutputTokens) => Some(BetaStopReason::MaxTokens),
            Some(ResponseIncompleteReason::ContentFilter) => Some(BetaStopReason::Refusal),
            None => Some(BetaStopReason::PauseTurn),
        },
        ResponseStatus::Failed | ResponseStatus::Cancelled => Some(BetaStopReason::PauseTurn),
        ResponseStatus::InProgress | ResponseStatus::Queued => None,
    }
}

fn map_usage(usage: &ResponseUsage) -> Option<BetaStreamUsage> {
    Some(BetaStreamUsage {
        input_tokens: Some(usage.input_tokens.max(0) as u32),
        output_tokens: Some(usage.output_tokens.max(0) as u32),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        server_tool_use: None,
    })
}

fn map_error(event: ResponseErrorEvent) -> ErrorDetail {
    ErrorDetail {
        r#type: ErrorType::Custom(event.code.unwrap_or_else(|| "error".to_string())),
        message: event.message,
    }
}
