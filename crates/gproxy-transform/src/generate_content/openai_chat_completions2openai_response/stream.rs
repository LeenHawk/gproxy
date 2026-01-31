use std::collections::BTreeMap;

use gproxy_protocol::openai::create_chat_completions::stream::CreateChatCompletionStreamResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionFunctionCallDelta,
    ChatCompletionMessageToolCallChunk, ChatCompletionRole, CompletionUsage,
};
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::stream::{
    ResponseCompletedEvent, ResponseCreatedEvent, ResponseFunctionCallArgumentsDeltaEvent,
    ResponseFunctionCallArgumentsDoneEvent, ResponseOutputItemAddedEvent,
    ResponseOutputItemDoneEvent, ResponseRefusalDeltaEvent, ResponseRefusalDoneEvent,
    ResponseStreamEvent, ResponseTextDeltaEvent, ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, MessageStatus, OutputItem,
    OutputMessage, OutputMessageContent, OutputMessageRole, OutputMessageType, RefusalContent,
    ResponseIncompleteDetails, ResponseIncompleteReason, ResponseStatus, ResponseUsage,
    ResponseUsageInputTokensDetails, ResponseUsageOutputTokensDetails,
};

#[derive(Debug, Clone)]
struct ChoiceState {
    output_index: i64,
    message_id: String,
    text: String,
    refusal: String,
}

#[derive(Debug, Clone)]
struct ToolCallState {
    output_index: i64,
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct OpenAIChatCompletionToResponseStreamState {
    id: String,
    model: String,
    created_at: i64,
    sequence_number: i64,
    created_sent: bool,
    next_output_index: i64,
    choices: BTreeMap<i64, ChoiceState>,
    tool_calls: BTreeMap<(i64, i64), ToolCallState>,
    output_items: BTreeMap<i64, OutputItem>,
    usage: Option<ResponseUsage>,
    finished: bool,
    pending_finish: Option<ChatCompletionFinishReason>,
}

impl OpenAIChatCompletionToResponseStreamState {
    pub fn new() -> Self {
        Self {
            id: "response".to_string(),
            model: "unknown".to_string(),
            created_at: 0,
            sequence_number: 0,
            created_sent: false,
            next_output_index: 0,
            choices: BTreeMap::new(),
            tool_calls: BTreeMap::new(),
            output_items: BTreeMap::new(),
            usage: None,
            finished: false,
            pending_finish: None,
        }
    }

    pub fn transform_event(
        &mut self,
        chunk: CreateChatCompletionStreamResponse,
    ) -> Vec<ResponseStreamEvent> {
        if self.finished {
            return Vec::new();
        }

        self.update_from_chunk(&chunk);
        let mut events = Vec::new();

        if !self.created_sent {
            self.created_sent = true;
            events.push(ResponseStreamEvent::Created(ResponseCreatedEvent {
                response: self.response_skeleton(ResponseStatus::InProgress, None, None, None),
                sequence_number: self.next_sequence(),
            }));
        }

        if let Some(usage) = &chunk.usage {
            self.usage = Some(map_usage(usage));
        }

        let mut finish_reason = None;
        for choice in chunk.choices {
            let choice_index = choice.index;
            let delta = choice.delta;

            if let Some(role) = delta.role
                && matches!(role, ChatCompletionRole::Assistant)
            {
                events.extend(self.ensure_message(choice_index));
            }

            if let Some(content) = delta.content {
                events.extend(self.emit_text(choice_index, content));
            } else if let Some(reasoning) = delta.reasoning_content {
                events.extend(self.emit_text(choice_index, reasoning));
            }

            if let Some(refusal) = delta.refusal {
                events.extend(self.emit_refusal(choice_index, refusal));
            }

            if let Some(function_call) = delta.function_call {
                events.extend(self.handle_function_call_delta(choice_index, function_call));
            }

            if let Some(tool_calls) = delta.tool_calls {
                for tool_call in tool_calls {
                    events.extend(self.handle_tool_call_delta(choice_index, tool_call));
                }
            }

            if let Some(reason) = choice.finish_reason {
                finish_reason = Some(reason);
            }
        }

        if let Some(reason) = finish_reason {
            self.pending_finish = Some(reason);
        }

        if self.pending_finish.is_some() && self.usage.is_some()
            && let Some(reason) = self.pending_finish.take() {
                events.extend(self.finish_response(reason));
            }

        events
    }

    fn ensure_message(&mut self, choice_index: i64) -> Vec<ResponseStreamEvent> {
        if self.choices.contains_key(&choice_index) {
            return Vec::new();
        }

        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let message_id = format!("message_{}", choice_index);

        let message = OutputItem::Message(OutputMessage {
            id: message_id.clone(),
            r#type: OutputMessageType::Message,
            role: OutputMessageRole::Assistant,
            content: Vec::new(),
            status: MessageStatus::InProgress,
        });

        self.output_items.insert(output_index, message.clone());
        self.choices.insert(
            choice_index,
            ChoiceState {
                output_index,
                message_id,
                text: String::new(),
                refusal: String::new(),
            },
        );

        vec![ResponseStreamEvent::OutputItemAdded(
            ResponseOutputItemAddedEvent {
                output_index,
                item: message,
                sequence_number: self.next_sequence(),
            },
        )]
    }

    fn emit_text(&mut self, choice_index: i64, text: String) -> Vec<ResponseStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut events = self.ensure_message(choice_index);
        if let Some(state) = self.choices.get_mut(&choice_index) {
            state.text.push_str(&text);
            events.push(ResponseStreamEvent::OutputTextDelta(
                ResponseTextDeltaEvent {
                    item_id: state.message_id.clone(),
                    output_index: state.output_index,
                    content_index: 0,
                    delta: text,
                    sequence_number: self.next_sequence(),
                    logprobs: Vec::new(),
                },
            ));
        }
        events
    }

    fn emit_refusal(&mut self, choice_index: i64, refusal: String) -> Vec<ResponseStreamEvent> {
        if refusal.is_empty() {
            return Vec::new();
        }

        let mut events = self.ensure_message(choice_index);
        if let Some(state) = self.choices.get_mut(&choice_index) {
            state.refusal.push_str(&refusal);
            events.push(ResponseStreamEvent::RefusalDelta(
                ResponseRefusalDeltaEvent {
                    item_id: state.message_id.clone(),
                    output_index: state.output_index,
                    content_index: 0,
                    delta: refusal,
                    sequence_number: self.next_sequence(),
                },
            ));
        }
        events
    }

    fn handle_function_call_delta(
        &mut self,
        choice_index: i64,
        function_call: ChatCompletionFunctionCallDelta,
    ) -> Vec<ResponseStreamEvent> {
        let tool_index = -1;
        let name = function_call
            .name
            .unwrap_or_else(|| "function_call".to_string());
        let arguments = function_call.arguments.unwrap_or_default();
        self.emit_tool_call_delta(choice_index, tool_index, None, name, arguments)
    }

    fn handle_tool_call_delta(
        &mut self,
        choice_index: i64,
        tool_call: ChatCompletionMessageToolCallChunk,
    ) -> Vec<ResponseStreamEvent> {
        let tool_index = tool_call.index;
        let name = tool_call
            .function
            .as_ref()
            .and_then(|function| function.name.clone())
            .unwrap_or_else(|| "tool_call".to_string());
        let arguments = tool_call
            .function
            .as_ref()
            .and_then(|function| function.arguments.clone())
            .unwrap_or_default();

        self.emit_tool_call_delta(choice_index, tool_index, tool_call.id, name, arguments)
    }

    fn emit_tool_call_delta(
        &mut self,
        choice_index: i64,
        tool_index: i64,
        id: Option<String>,
        name: String,
        arguments: String,
    ) -> Vec<ResponseStreamEvent> {
        let key = (choice_index, tool_index);
        let mut events = Vec::new();

        let state = if let Some(state) = self.tool_calls.get_mut(&key) {
            if !name.is_empty() {
                state.name = name;
            }
            state
        } else {
            let output_index = self.next_output_index;
            self.next_output_index += 1;
            let item_id = id.unwrap_or_else(|| format!("tool_{}_{}", choice_index, tool_index));
            let state = ToolCallState {
                output_index,
                id: item_id.clone(),
                name: name.clone(),
                arguments: String::new(),
            };
            let item = OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(item_id.clone()),
                call_id: item_id.clone(),
                name,
                arguments: String::new(),
                status: Some(FunctionCallItemStatus::InProgress),
            });
            events.push(ResponseStreamEvent::OutputItemAdded(
                ResponseOutputItemAddedEvent {
                    output_index,
                    item: item.clone(),
                    sequence_number: self.next_sequence(),
                },
            ));
            self.output_items.insert(output_index, item);
            self.tool_calls.insert(key, state);
            self.tool_calls.get_mut(&key).expect("tool state")
        };

        if !arguments.is_empty() {
            state.arguments.push_str(&arguments);
            events.push(ResponseStreamEvent::FunctionCallArgumentsDelta(
                ResponseFunctionCallArgumentsDeltaEvent {
                    item_id: state.id.clone(),
                    output_index: state.output_index,
                    delta: arguments,
                    sequence_number: self.next_sequence(),
                },
            ));
        }

        events
    }

    fn finish_response(
        &mut self,
        finish_reason: ChatCompletionFinishReason,
    ) -> Vec<ResponseStreamEvent> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut events = Vec::new();
        let (status, incomplete_details) = map_finish_reason(finish_reason);

        let choice_states = self.choices.values().cloned().collect::<Vec<ChoiceState>>();
        for state in choice_states {
            if !state.refusal.is_empty() {
                events.push(ResponseStreamEvent::RefusalDone(ResponseRefusalDoneEvent {
                    item_id: state.message_id.clone(),
                    output_index: state.output_index,
                    content_index: 0,
                    refusal: state.refusal.clone(),
                    sequence_number: self.next_sequence(),
                }));
            } else if !state.text.is_empty() {
                events.push(ResponseStreamEvent::OutputTextDone(ResponseTextDoneEvent {
                    item_id: state.message_id.clone(),
                    output_index: state.output_index,
                    content_index: 0,
                    text: state.text.clone(),
                    sequence_number: self.next_sequence(),
                    logprobs: Vec::new(),
                }));
            }

            let content = if !state.refusal.is_empty() {
                vec![OutputMessageContent::Refusal(RefusalContent {
                    refusal: state.refusal.clone(),
                })]
            } else if !state.text.is_empty() {
                vec![OutputMessageContent::OutputText(
                    gproxy_protocol::openai::create_response::types::OutputTextContent {
                        text: state.text.clone(),
                        annotations: Vec::new(),
                        logprobs: None,
                    },
                )]
            } else {
                Vec::new()
            };

            let message_status = match finish_reason {
                ChatCompletionFinishReason::Length => MessageStatus::Incomplete,
                ChatCompletionFinishReason::ContentFilter => MessageStatus::Incomplete,
                _ => MessageStatus::Completed,
            };

            let message = OutputItem::Message(OutputMessage {
                id: state.message_id.clone(),
                r#type: OutputMessageType::Message,
                role: OutputMessageRole::Assistant,
                content,
                status: message_status,
            });

            events.push(ResponseStreamEvent::OutputItemDone(
                ResponseOutputItemDoneEvent {
                    output_index: state.output_index,
                    item: message.clone(),
                    sequence_number: self.next_sequence(),
                },
            ));
            self.output_items.insert(state.output_index, message);
        }

        let tool_states = self
            .tool_calls
            .values()
            .cloned()
            .collect::<Vec<ToolCallState>>();
        for state in tool_states {
            events.push(ResponseStreamEvent::FunctionCallArgumentsDone(
                ResponseFunctionCallArgumentsDoneEvent {
                    item_id: state.id.clone(),
                    name: state.name.clone(),
                    output_index: state.output_index,
                    arguments: state.arguments.clone(),
                    sequence_number: self.next_sequence(),
                },
            ));

            let item = OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(state.id.clone()),
                call_id: state.id.clone(),
                name: state.name.clone(),
                arguments: state.arguments.clone(),
                status: Some(FunctionCallItemStatus::Completed),
            });

            events.push(ResponseStreamEvent::OutputItemDone(
                ResponseOutputItemDoneEvent {
                    output_index: state.output_index,
                    item: item.clone(),
                    sequence_number: self.next_sequence(),
                },
            ));
            self.output_items.insert(state.output_index, item);
        }

        let output = self
            .output_items
            .values()
            .cloned()
            .collect::<Vec<OutputItem>>();

        events.push(ResponseStreamEvent::Completed(ResponseCompletedEvent {
            response: self.response_skeleton(
                status,
                self.usage.clone(),
                incomplete_details,
                Some(output),
            ),
            sequence_number: self.next_sequence(),
        }));

        events
    }

    fn update_from_chunk(&mut self, chunk: &CreateChatCompletionStreamResponse) {
        self.id = chunk.id.clone();
        self.model = chunk.model.clone();
        self.created_at = chunk.created;
    }

    fn response_skeleton(
        &self,
        status: ResponseStatus,
        usage: Option<ResponseUsage>,
        incomplete_details: Option<ResponseIncompleteDetails>,
        output: Option<Vec<OutputItem>>,
    ) -> Response {
        let output = output.unwrap_or_default();
        let output_text = extract_output_text(&output);

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
            output,
            output_text,
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
        let next = self.sequence_number;
        self.sequence_number += 1;
        next
    }
}

impl Default for OpenAIChatCompletionToResponseStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn map_finish_reason(
    reason: ChatCompletionFinishReason,
) -> (ResponseStatus, Option<ResponseIncompleteDetails>) {
    match reason {
        ChatCompletionFinishReason::Length => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::MaxOutputTokens,
            }),
        ),
        ChatCompletionFinishReason::ContentFilter => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::ContentFilter,
            }),
        ),
        _ => (ResponseStatus::Completed, None),
    }
}

fn map_usage(usage: &CompletionUsage) -> ResponseUsage {
    let cached_tokens = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens)
        .unwrap_or(0);
    let reasoning_tokens = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or(0);

    ResponseUsage {
        input_tokens: usage.prompt_tokens,
        input_tokens_details: ResponseUsageInputTokensDetails { cached_tokens },
        output_tokens: usage.completion_tokens,
        output_tokens_details: ResponseUsageOutputTokensDetails { reasoning_tokens },
        total_tokens: usage.total_tokens,
    }
}

fn extract_output_text(output: &[OutputItem]) -> Option<String> {
    for item in output {
        if let OutputItem::Message(message) = item {
            for content in &message.content {
                if let OutputMessageContent::OutputText(text) = content
                    && !text.text.is_empty()
                {
                    return Some(text.text.clone());
                }
            }
        }
    }
    None
}
