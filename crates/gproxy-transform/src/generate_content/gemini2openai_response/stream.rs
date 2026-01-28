use std::collections::BTreeMap;

use gproxy_protocol::gemini::count_tokens::types::Part as GeminiPart;
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::stream::{
    ResponseCompletedEvent, ResponseCreatedEvent, ResponseFunctionCallArgumentsDeltaEvent,
    ResponseFunctionCallArgumentsDoneEvent, ResponseOutputItemAddedEvent,
    ResponseOutputItemDoneEvent, ResponseStreamEvent, ResponseTextDeltaEvent,
    ResponseTextDoneEvent,
};
use gproxy_protocol::openai::create_response::types::{
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, MessageStatus, OutputItem,
    OutputMessage, OutputMessageContent, OutputMessageRole, OutputMessageType, OutputTextContent,
    ResponseIncompleteDetails, ResponseIncompleteReason, ResponseStatus, ResponseUsage,
    ResponseUsageInputTokensDetails, ResponseUsageOutputTokensDetails,
};

#[derive(Debug, Clone)]
struct MessageState {
    output_index: i64,
    message_id: String,
    text: String,
}

#[derive(Debug, Clone)]
struct ToolState {
    output_index: i64,
    id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct GeminiToOpenAIResponseStreamState {
    id: String,
    model: String,
    created_at: i64,
    sequence_number: i64,
    created_sent: bool,
    next_output_index: i64,
    message_states: BTreeMap<i64, MessageState>,
    tool_states: BTreeMap<String, ToolState>,
    output_items: BTreeMap<i64, OutputItem>,
    tool_counter: i64,
    usage: Option<ResponseUsage>,
    finished: bool,
}

impl GeminiToOpenAIResponseStreamState {
    pub fn new() -> Self {
        Self {
            id: "response".to_string(),
            model: "unknown".to_string(),
            created_at: 0,
            sequence_number: 0,
            created_sent: false,
            next_output_index: 0,
            message_states: BTreeMap::new(),
            tool_states: BTreeMap::new(),
            output_items: BTreeMap::new(),
            tool_counter: 0,
            usage: None,
            finished: false,
        }
    }

    pub fn transform_response(
        &mut self,
        response: GenerateContentResponse,
    ) -> Vec<ResponseStreamEvent> {
        if self.finished {
            return Vec::new();
        }

        self.update_from_response(&response);

        let mut events = Vec::new();
        if !self.created_sent {
            self.created_sent = true;
            events.push(ResponseStreamEvent::Created(ResponseCreatedEvent {
                response: self.response_skeleton(ResponseStatus::InProgress, None, None, None),
                sequence_number: self.next_sequence(),
            }));
        }

        if let Some(usage) = &response.usage_metadata {
            self.usage = Some(map_usage(usage));
        }

        let mut finish_reason = None;
        for (index, candidate) in response.candidates.iter().enumerate() {
            let candidate_index = candidate
                .index
                .map(|value| value as i64)
                .unwrap_or(index as i64);
            events.extend(self.handle_candidate(candidate_index, &candidate.content.parts));
            if let Some(reason) = candidate.finish_reason {
                finish_reason = Some(reason);
            }
        }

        if let Some(reason) = finish_reason {
            events.extend(self.finish_response(reason));
        }

        events
    }

    fn handle_candidate(
        &mut self,
        candidate_index: i64,
        parts: &[GeminiPart],
    ) -> Vec<ResponseStreamEvent> {
        let mut events = Vec::new();
        for part in parts {
            events.extend(self.handle_part(candidate_index, part));
        }
        events
    }

    fn handle_part(&mut self, candidate_index: i64, part: &GeminiPart) -> Vec<ResponseStreamEvent> {
        let mut events = Vec::new();

        if let Some(text) = part.text.clone()
            && !text.is_empty()
        {
            events.extend(self.emit_text(candidate_index, text));
        }

        if let Some(function_call) = &part.function_call {
            events.extend(self.emit_function_call(candidate_index, function_call));
        }

        if let Some(function_response) = &part.function_response
            && let Ok(text) = serde_json::to_string(function_response)
            && !text.is_empty()
        {
            events.extend(self.emit_text(candidate_index, text));
        }

        if let Some(code) = &part.executable_code
            && let Ok(text) = serde_json::to_string(code)
            && !text.is_empty()
        {
            events.extend(self.emit_text(candidate_index, text));
        }

        if let Some(result) = &part.code_execution_result
            && let Ok(text) = serde_json::to_string(result)
            && !text.is_empty()
        {
            events.extend(self.emit_text(candidate_index, text));
        }

        if part.inline_data.is_some() {
            events.extend(self.emit_text(candidate_index, "[inline_data]".to_string()));
        }

        if let Some(file_data) = &part.file_data {
            events
                .extend(self.emit_text(candidate_index, format!("[file:{}]", file_data.file_uri)));
        }

        events
    }

    fn ensure_message(&mut self, candidate_index: i64) -> Vec<ResponseStreamEvent> {
        if self.message_states.contains_key(&candidate_index) {
            return Vec::new();
        }

        let output_index = self.next_output_index;
        self.next_output_index += 1;
        let message_id = format!("message_{}", candidate_index);

        let message = OutputItem::Message(OutputMessage {
            id: message_id.clone(),
            r#type: OutputMessageType::Message,
            role: OutputMessageRole::Assistant,
            content: Vec::new(),
            status: MessageStatus::InProgress,
        });

        self.output_items.insert(output_index, message.clone());
        self.message_states.insert(
            candidate_index,
            MessageState {
                output_index,
                message_id,
                text: String::new(),
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

    fn emit_text(&mut self, candidate_index: i64, text: String) -> Vec<ResponseStreamEvent> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut events = self.ensure_message(candidate_index);
        if let Some(state) = self.message_states.get_mut(&candidate_index) {
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

    fn emit_function_call(
        &mut self,
        candidate_index: i64,
        call: &gproxy_protocol::gemini::count_tokens::types::FunctionCall,
    ) -> Vec<ResponseStreamEvent> {
        let call_id = call
            .id
            .clone()
            .unwrap_or_else(|| self.next_tool_id(candidate_index));
        let args = call
            .args
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .unwrap_or_else(|| "{}".to_string());

        let mut events = Vec::new();
        let state = if let Some(state) = self.tool_states.get_mut(&call_id) {
            state
        } else {
            let output_index = self.next_output_index;
            self.next_output_index += 1;

            let item = OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(call_id.clone()),
                call_id: call_id.clone(),
                name: call.name.clone(),
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
            self.tool_states.insert(
                call_id.clone(),
                ToolState {
                    output_index,
                    id: call_id.clone(),
                    name: call.name.clone(),
                    arguments: String::new(),
                },
            );
            self.tool_states.get_mut(&call_id).expect("tool state")
        };

        let delta = compute_delta(Some(&state.arguments), &args);
        state.arguments = args;
        if !delta.is_empty() {
            events.push(ResponseStreamEvent::FunctionCallArgumentsDelta(
                ResponseFunctionCallArgumentsDeltaEvent {
                    item_id: state.id.clone(),
                    output_index: state.output_index,
                    delta,
                    sequence_number: self.next_sequence(),
                },
            ));
        }

        events
    }

    fn finish_response(&mut self, finish_reason: FinishReason) -> Vec<ResponseStreamEvent> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        let mut events = Vec::new();
        let (status, incomplete_details) = map_finish_reason(finish_reason);

        let message_states = self
            .message_states
            .values()
            .cloned()
            .collect::<Vec<MessageState>>();
        for state in message_states {
            if !state.text.is_empty() {
                events.push(ResponseStreamEvent::OutputTextDone(ResponseTextDoneEvent {
                    item_id: state.message_id.clone(),
                    output_index: state.output_index,
                    content_index: 0,
                    text: state.text.clone(),
                    sequence_number: self.next_sequence(),
                    logprobs: Vec::new(),
                }));
            }

            let content = if state.text.is_empty() {
                Vec::new()
            } else {
                vec![OutputMessageContent::OutputText(OutputTextContent {
                    text: state.text.clone(),
                    annotations: Vec::new(),
                    logprobs: None,
                })]
            };

            let message = OutputItem::Message(OutputMessage {
                id: state.message_id.clone(),
                r#type: OutputMessageType::Message,
                role: OutputMessageRole::Assistant,
                content,
                status: if matches!(status, ResponseStatus::Incomplete) {
                    MessageStatus::Incomplete
                } else {
                    MessageStatus::Completed
                },
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
            .tool_states
            .values()
            .cloned()
            .collect::<Vec<ToolState>>();
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

    fn update_from_response(&mut self, response: &GenerateContentResponse) {
        if let Some(id) = response.response_id.clone() {
            self.id = id;
        }
        if let Some(model) = response.model_version.clone().or_else(|| {
            response
                .model_status
                .as_ref()
                .map(|status| format!("{:?}", status.model_stage))
        }) {
            self.model = model.strip_prefix("models/").unwrap_or(&model).to_string();
        }
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

    fn next_tool_id(&mut self, candidate_index: i64) -> String {
        let id = format!("tool_call_{}_{}", candidate_index, self.tool_counter);
        self.tool_counter += 1;
        id
    }
}

impl Default for GeminiToOpenAIResponseStreamState {
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

fn map_finish_reason(reason: FinishReason) -> (ResponseStatus, Option<ResponseIncompleteDetails>) {
    match reason {
        FinishReason::MaxTokens => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::MaxOutputTokens,
            }),
        ),
        FinishReason::Safety
        | FinishReason::Blocklist
        | FinishReason::ProhibitedContent
        | FinishReason::Spii
        | FinishReason::ImageSafety
        | FinishReason::ImageProhibitedContent
        | FinishReason::ImageRecitation
        | FinishReason::NoImage
        | FinishReason::Recitation => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::ContentFilter,
            }),
        ),
        _ => (ResponseStatus::Completed, None),
    }
}

fn map_usage(usage: &UsageMetadata) -> ResponseUsage {
    let input_tokens = usage.prompt_token_count.unwrap_or(0) as i64;
    let output_tokens = usage.candidates_token_count.unwrap_or(0) as i64;
    let total_tokens = usage
        .total_token_count
        .map(|value| value as i64)
        .unwrap_or_else(|| input_tokens + output_tokens);

    ResponseUsage {
        input_tokens,
        input_tokens_details: ResponseUsageInputTokensDetails {
            cached_tokens: usage.cached_content_token_count.unwrap_or(0) as i64,
        },
        output_tokens,
        output_tokens_details: ResponseUsageOutputTokensDetails {
            reasoning_tokens: usage.thoughts_token_count.unwrap_or(0) as i64,
        },
        total_tokens,
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
