use std::collections::BTreeMap;

use gproxy_protocol::openai::create_chat_completions::stream::{
    ChatCompletionChunkObjectType, ChatCompletionStreamChoice, CreateChatCompletionStreamResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCallChunk,
    ChatCompletionMessageToolCallChunkFunction, ChatCompletionRole,
    ChatCompletionStreamResponseDelta, ChatCompletionToolCallChunkType, CompletionTokensDetails,
    CompletionUsage, PromptTokensDetails,
};
use gproxy_protocol::openai::create_response::response::Response;
use gproxy_protocol::openai::create_response::stream::{
    ResponseCompletedEvent, ResponseFunctionCallArgumentsDeltaEvent,
    ResponseFunctionCallArgumentsDoneEvent, ResponseMCPCallArgumentsDeltaEvent,
    ResponseMCPCallArgumentsDoneEvent, ResponseOutputItemAddedEvent, ResponseOutputItemDoneEvent,
    ResponseRefusalDeltaEvent, ResponseRefusalDoneEvent, ResponseStreamEvent,
    ResponseTextDeltaEvent, ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    CustomToolCall, FunctionToolCall, MCPToolCall, OutputItem, ResponseIncompleteDetails,
    ResponseIncompleteReason, ResponseStatus, ResponseUsage,
};

#[derive(Debug, Clone)]
struct ToolCallState {
    index: i64,
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct OpenAIResponseToChatCompletionStreamState {
    id: String,
    model: String,
    created: i64,
    role_sent: bool,
    tool_calls: BTreeMap<i64, ToolCallState>,
    next_tool_index: i64,
    text_buffers: BTreeMap<(i64, i64), String>,
    refusal_buffers: BTreeMap<(i64, i64), String>,
    saw_tool_calls: bool,
    saw_refusal: bool,
    status: Option<ResponseStatus>,
    incomplete_details: Option<ResponseIncompleteDetails>,
    usage: Option<ResponseUsage>,
    finished: bool,
}

impl OpenAIResponseToChatCompletionStreamState {
    pub fn new() -> Self {
        Self {
            id: "response".to_string(),
            model: "unknown".to_string(),
            created: 0,
            role_sent: false,
            tool_calls: BTreeMap::new(),
            next_tool_index: 0,
            text_buffers: BTreeMap::new(),
            refusal_buffers: BTreeMap::new(),
            saw_tool_calls: false,
            saw_refusal: false,
            status: None,
            incomplete_details: None,
            usage: None,
            finished: false,
        }
    }

    pub fn transform_event(
        &mut self,
        event: ResponseStreamEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        match event {
            ResponseStreamEvent::Created(event) => {
                self.update_from_response(&event.response);
                Vec::new()
            }
            ResponseStreamEvent::InProgress(event) => {
                self.update_from_response(&event.response);
                Vec::new()
            }
            ResponseStreamEvent::Completed(event) => self.finish_from_response(event),
            ResponseStreamEvent::Failed(event) => {
                self.finish_from_response(ResponseCompletedEvent {
                    response: event.response,
                    sequence_number: event.sequence_number,
                })
            }
            ResponseStreamEvent::Incomplete(event) => {
                self.finish_from_response(ResponseCompletedEvent {
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
            ResponseStreamEvent::Error(_) => Vec::new(),
            _ => Vec::new(),
        }
    }

    fn handle_output_item_added(
        &mut self,
        event: ResponseOutputItemAddedEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        match event.item {
            OutputItem::Function(function) => {
                self.emit_tool_call(event.output_index, function, None)
            }
            OutputItem::CustomToolCall(custom) => {
                self.emit_custom_tool_call(event.output_index, custom)
            }
            OutputItem::MCPCall(mcp) => self.emit_mcp_tool_call(event.output_index, mcp),
            _ => Vec::new(),
        }
    }

    fn handle_output_item_done(
        &mut self,
        event: ResponseOutputItemDoneEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        match event.item {
            OutputItem::Function(function) => {
                self.emit_tool_call_done_for_function(event.output_index, &function)
            }
            OutputItem::CustomToolCall(custom) => {
                self.emit_custom_tool_call_done(event.output_index, &custom)
            }
            OutputItem::MCPCall(mcp) => self.emit_mcp_tool_call_done(event.output_index, &mcp),
            _ => Vec::new(),
        }
    }

    fn handle_text_delta(
        &mut self,
        event: ResponseTextDeltaEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        if event.delta.is_empty() {
            return Vec::new();
        }
        self.text_buffers
            .entry((event.output_index, event.content_index))
            .and_modify(|value| value.push_str(&event.delta))
            .or_insert_with(|| event.delta.clone());

        let role = self.take_role();
        self.emit_delta(ChatCompletionStreamResponseDelta {
            content: Some(event.delta),
            reasoning_content: None,
            function_call: None,
            tool_calls: None,
            role,
            refusal: None,
            obfuscation: None,
        })
    }

    fn handle_text_done(
        &mut self,
        event: ResponseTextDoneEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let key = (event.output_index, event.content_index);
        let delta = compute_delta(self.text_buffers.get(&key), &event.text);
        self.text_buffers.insert(key, event.text);

        if delta.is_empty() {
            Vec::new()
        } else {
            let role = self.take_role();
            self.emit_delta(ChatCompletionStreamResponseDelta {
                content: Some(delta),
                reasoning_content: None,
                function_call: None,
                tool_calls: None,
                role,
                refusal: None,
                obfuscation: None,
            })
        }
    }

    fn handle_refusal_delta(
        &mut self,
        event: ResponseRefusalDeltaEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        if event.delta.is_empty() {
            return Vec::new();
        }
        self.saw_refusal = true;
        self.refusal_buffers
            .entry((event.output_index, event.content_index))
            .and_modify(|value| value.push_str(&event.delta))
            .or_insert_with(|| event.delta.clone());

        let role = self.take_role();
        self.emit_delta(ChatCompletionStreamResponseDelta {
            content: None,
            reasoning_content: None,
            function_call: None,
            tool_calls: None,
            role,
            refusal: Some(event.delta),
            obfuscation: None,
        })
    }

    fn handle_refusal_done(
        &mut self,
        event: ResponseRefusalDoneEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        self.saw_refusal = true;
        let key = (event.output_index, event.content_index);
        let delta = compute_delta(self.refusal_buffers.get(&key), &event.refusal);
        self.refusal_buffers.insert(key, event.refusal);

        if delta.is_empty() {
            Vec::new()
        } else {
            let role = self.take_role();
            self.emit_delta(ChatCompletionStreamResponseDelta {
                content: None,
                reasoning_content: None,
                function_call: None,
                tool_calls: None,
                role,
                refusal: Some(delta),
                obfuscation: None,
            })
        }
    }

    fn handle_function_call_delta(
        &mut self,
        event: ResponseFunctionCallArgumentsDeltaEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let (index, id, name, arguments) = {
            let state = self.ensure_tool_state(event.output_index, Some(event.item_id), None);
            state.arguments.push_str(&event.delta);
            (
                state.index,
                state.id.clone(),
                state.name.clone(),
                event.delta,
            )
        };
        self.emit_tool_chunk_with(index, id, name, Some(arguments))
    }

    fn handle_function_call_done(
        &mut self,
        event: ResponseFunctionCallArgumentsDoneEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let (index, id, name, delta) = {
            let state =
                self.ensure_tool_state(event.output_index, Some(event.item_id), Some(event.name));
            let delta = compute_delta(Some(&state.arguments), &event.arguments);
            state.arguments = event.arguments;
            (state.index, state.id.clone(), state.name.clone(), delta)
        };
        if delta.is_empty() {
            Vec::new()
        } else {
            self.emit_tool_chunk_with(index, id, name, Some(delta))
        }
    }

    fn handle_mcp_call_delta(
        &mut self,
        event: ResponseMCPCallArgumentsDeltaEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let (index, id, name, arguments) = {
            let state = self.ensure_tool_state(event.output_index, Some(event.item_id), None);
            state.arguments.push_str(&event.delta);
            (
                state.index,
                state.id.clone(),
                state.name.clone(),
                event.delta,
            )
        };
        self.emit_tool_chunk_with(index, id, name, Some(arguments))
    }

    fn handle_mcp_call_done(
        &mut self,
        event: ResponseMCPCallArgumentsDoneEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let (index, id, name, delta) = {
            let state = self.ensure_tool_state(event.output_index, Some(event.item_id), None);
            let delta = compute_delta(Some(&state.arguments), &event.arguments);
            state.arguments = event.arguments;
            (state.index, state.id.clone(), state.name.clone(), delta)
        };
        if delta.is_empty() {
            Vec::new()
        } else {
            self.emit_tool_chunk_with(index, id, name, Some(delta))
        }
    }

    fn emit_tool_call(
        &mut self,
        output_index: i64,
        function: FunctionToolCall,
        explicit_id: Option<String>,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let id = explicit_id
            .or_else(|| function.id.clone())
            .or_else(|| Some(function.call_id.clone()));
        let (index, id, name, arguments) = {
            let state = self.ensure_tool_state(output_index, id, Some(function.name.clone()));
            if !function.arguments.is_empty() {
                state.arguments = function.arguments.clone();
            }
            (
                state.index,
                state.id.clone(),
                state.name.clone(),
                if function.arguments.is_empty() {
                    None
                } else {
                    Some(function.arguments.clone())
                },
            )
        };
        self.emit_tool_chunk_with(index, id, name, arguments)
    }

    fn emit_custom_tool_call(
        &mut self,
        output_index: i64,
        custom: CustomToolCall,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let id = custom.id.clone().or_else(|| Some(custom.call_id.clone()));
        let (index, id, name, arguments) = {
            let state = self.ensure_tool_state(output_index, id, Some(custom.name.clone()));
            if !custom.input.is_empty() {
                state.arguments = custom.input.clone();
            }
            (
                state.index,
                state.id.clone(),
                state.name.clone(),
                if custom.input.is_empty() {
                    None
                } else {
                    Some(custom.input.clone())
                },
            )
        };
        self.emit_tool_chunk_with(index, id, name, arguments)
    }

    fn emit_mcp_tool_call(
        &mut self,
        output_index: i64,
        mcp: MCPToolCall,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let id = Some(mcp.id.clone());
        let name = format!("mcp:{}:{}", mcp.server_label, mcp.name);
        let (index, id, name, arguments) = {
            let state = self.ensure_tool_state(output_index, id, Some(name));
            if !mcp.arguments.is_empty() {
                state.arguments = mcp.arguments.clone();
            }
            (
                state.index,
                state.id.clone(),
                state.name.clone(),
                if mcp.arguments.is_empty() {
                    None
                } else {
                    Some(mcp.arguments.clone())
                },
            )
        };
        self.emit_tool_chunk_with(index, id, name, arguments)
    }

    fn emit_tool_call_done(
        &mut self,
        output_index: i64,
        id: Option<String>,
        name: Option<String>,
        arguments: String,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let (index, id, name, delta) = {
            let state = self.ensure_tool_state(output_index, id, name);
            let delta = compute_delta(Some(&state.arguments), &arguments);
            state.arguments = arguments;
            (state.index, state.id.clone(), state.name.clone(), delta)
        };
        if delta.is_empty() {
            Vec::new()
        } else {
            self.emit_tool_chunk_with(index, id, name, Some(delta))
        }
    }

    fn emit_tool_call_done_for_function(
        &mut self,
        output_index: i64,
        function: &FunctionToolCall,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        self.emit_tool_call_done(
            output_index,
            function
                .id
                .clone()
                .or_else(|| Some(function.call_id.clone())),
            Some(function.name.clone()),
            function.arguments.clone(),
        )
    }

    fn emit_custom_tool_call_done(
        &mut self,
        output_index: i64,
        custom: &CustomToolCall,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        self.emit_tool_call_done(
            output_index,
            custom.id.clone().or_else(|| Some(custom.call_id.clone())),
            Some(custom.name.clone()),
            custom.input.clone(),
        )
    }

    fn emit_mcp_tool_call_done(
        &mut self,
        output_index: i64,
        mcp: &MCPToolCall,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let name = format!("mcp:{}:{}", mcp.server_label, mcp.name);
        self.emit_tool_call_done(
            output_index,
            Some(mcp.id.clone()),
            Some(name),
            mcp.arguments.clone(),
        )
    }

    fn finish_from_response(
        &mut self,
        event: ResponseCompletedEvent,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        self.update_from_response(&event.response);

        let finish_reason = self.resolve_finish_reason();
        let mut delta = ChatCompletionStreamResponseDelta {
            content: None,
            reasoning_content: None,
            function_call: None,
            tool_calls: None,
            role: self.take_role(),
            refusal: None,
            obfuscation: None,
        };
        if finish_reason == ChatCompletionFinishReason::ToolCalls && !self.saw_tool_calls {
            delta.role = None;
        }

        vec![CreateChatCompletionStreamResponse {
            id: self.id.clone(),
            object: ChatCompletionChunkObjectType::ChatCompletionChunk,
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChatCompletionStreamChoice {
                index: 0,
                delta,
                logprobs: None,
                finish_reason: Some(finish_reason),
            }],
            usage: self.usage.as_ref().map(map_usage),
            service_tier: None,
            system_fingerprint: None,
        }]
    }

    fn resolve_finish_reason(&self) -> ChatCompletionFinishReason {
        if self.saw_tool_calls {
            return ChatCompletionFinishReason::ToolCalls;
        }
        if self.saw_refusal {
            return ChatCompletionFinishReason::ContentFilter;
        }
        if let Some(details) = &self.incomplete_details {
            return match details.reason {
                ResponseIncompleteReason::MaxOutputTokens => ChatCompletionFinishReason::Length,
                ResponseIncompleteReason::ContentFilter => {
                    ChatCompletionFinishReason::ContentFilter
                }
            };
        }
        ChatCompletionFinishReason::Stop
    }

    fn update_from_response(&mut self, response: &Response) {
        self.id = response.id.clone();
        self.model = response.model.clone();
        self.created = response.created_at;
        self.status = response.status;
        self.incomplete_details = response.incomplete_details.clone();
        if let Some(usage) = &response.usage {
            self.usage = Some(usage.clone());
        }
    }

    fn emit_delta(
        &mut self,
        delta: ChatCompletionStreamResponseDelta,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        vec![CreateChatCompletionStreamResponse {
            id: self.id.clone(),
            object: ChatCompletionChunkObjectType::ChatCompletionChunk,
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChatCompletionStreamChoice {
                index: 0,
                delta,
                logprobs: None,
                finish_reason: None,
            }],
            usage: None,
            service_tier: None,
            system_fingerprint: None,
        }]
    }

    fn ensure_tool_state(
        &mut self,
        output_index: i64,
        id: Option<String>,
        name: Option<String>,
    ) -> &mut ToolCallState {
        if !self.tool_calls.contains_key(&output_index) {
            let index = self.next_tool_index;
            self.next_tool_index += 1;
            self.tool_calls.insert(
                output_index,
                ToolCallState {
                    index,
                    id: id.clone(),
                    name: name.clone(),
                    arguments: String::new(),
                },
            );
        }

        let state = self.tool_calls.get_mut(&output_index).expect("tool state");
        if state.id.is_none() {
            state.id = id;
        }
        if state.name.is_none() {
            state.name = name;
        }
        state
    }

    fn emit_tool_chunk_with(
        &mut self,
        index: i64,
        id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        self.saw_tool_calls = true;
        let function = ChatCompletionMessageToolCallChunkFunction { name, arguments };

        if function.name.is_none() && function.arguments.is_none() {
            return Vec::new();
        }

        let chunk = ChatCompletionMessageToolCallChunk {
            index,
            id,
            r#type: Some(ChatCompletionToolCallChunkType::Function),
            function: Some(function),
        };

        let role = self.take_role();
        self.emit_delta(ChatCompletionStreamResponseDelta {
            content: None,
            reasoning_content: None,
            function_call: None,
            tool_calls: Some(vec![chunk]),
            role,
            refusal: None,
            obfuscation: None,
        })
    }

    fn take_role(&mut self) -> Option<ChatCompletionRole> {
        if self.role_sent {
            None
        } else {
            self.role_sent = true;
            Some(ChatCompletionRole::Assistant)
        }
    }
}

impl Default for OpenAIResponseToChatCompletionStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn compute_delta(previous: Option<&String>, full: &str) -> String {
    match previous {
        Some(prev) if full.starts_with(prev) => full[prev.len()..].to_string(),
        _ => full.to_string(),
    }
}

fn map_usage(usage: &ResponseUsage) -> CompletionUsage {
    CompletionUsage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        completion_tokens_details: Some(CompletionTokensDetails {
            accepted_prediction_tokens: None,
            audio_tokens: None,
            reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
            rejected_prediction_tokens: None,
        }),
        prompt_tokens_details: Some(PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: Some(usage.input_tokens_details.cached_tokens),
        }),
    }
}
