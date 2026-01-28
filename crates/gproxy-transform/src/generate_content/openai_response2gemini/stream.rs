use std::collections::BTreeMap;

use gproxy_protocol::gemini::count_tokens::types::{
    Content as GeminiContent, ContentRole as GeminiContentRole, FunctionCall as GeminiFunctionCall,
    Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
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
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolKind {
    Function,
    Mcp,
    Custom,
}

#[derive(Debug, Clone)]
struct ToolState {
    id: String,
    name: String,
    kind: ToolKind,
    arguments: String,
    server_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenAIResponseToGeminiStreamState {
    response_id: String,
    model_version: String,
    text_buffers: BTreeMap<(i64, i64), String>,
    refusal_buffers: BTreeMap<(i64, i64), String>,
    tool_states: BTreeMap<i64, ToolState>,
    usage: Option<ResponseUsage>,
    saw_refusal: bool,
    finished: bool,
}

impl OpenAIResponseToGeminiStreamState {
    pub fn new() -> Self {
        Self {
            response_id: "response".to_string(),
            model_version: "models/unknown".to_string(),
            text_buffers: BTreeMap::new(),
            refusal_buffers: BTreeMap::new(),
            tool_states: BTreeMap::new(),
            usage: None,
            saw_refusal: false,
            finished: false,
        }
    }

    pub fn transform_event(&mut self, event: ResponseStreamEvent) -> Vec<GenerateContentResponse> {
        if self.finished {
            return Vec::new();
        }

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
    ) -> Vec<GenerateContentResponse> {
        match event.item {
            OutputItem::Function(function) => self.emit_function_call(event.output_index, function),
            OutputItem::CustomToolCall(custom) => self.emit_custom_call(event.output_index, custom),
            OutputItem::MCPCall(mcp) => self.emit_mcp_call(event.output_index, mcp),
            _ => Vec::new(),
        }
    }

    fn handle_output_item_done(
        &mut self,
        event: ResponseOutputItemDoneEvent,
    ) -> Vec<GenerateContentResponse> {
        if let Some(state) = self.tool_states.get(&event.output_index) {
            return self.emit_tool_state(state);
        }
        Vec::new()
    }

    fn handle_text_delta(&mut self, event: ResponseTextDeltaEvent) -> Vec<GenerateContentResponse> {
        if event.delta.is_empty() {
            return Vec::new();
        }
        self.text_buffers
            .entry((event.output_index, event.content_index))
            .and_modify(|value| value.push_str(&event.delta))
            .or_insert_with(|| event.delta.clone());
        self.emit_parts(vec![text_part(event.delta)])
    }

    fn handle_text_done(&mut self, event: ResponseTextDoneEvent) -> Vec<GenerateContentResponse> {
        let key = (event.output_index, event.content_index);
        let delta = compute_delta(self.text_buffers.get(&key), &event.text);
        self.text_buffers.insert(key, event.text);
        if delta.is_empty() {
            Vec::new()
        } else {
            self.emit_parts(vec![text_part(delta)])
        }
    }

    fn handle_refusal_delta(
        &mut self,
        event: ResponseRefusalDeltaEvent,
    ) -> Vec<GenerateContentResponse> {
        if event.delta.is_empty() {
            return Vec::new();
        }
        self.saw_refusal = true;
        self.refusal_buffers
            .entry((event.output_index, event.content_index))
            .and_modify(|value| value.push_str(&event.delta))
            .or_insert_with(|| event.delta.clone());
        self.emit_parts(vec![text_part(event.delta)])
    }

    fn handle_refusal_done(
        &mut self,
        event: ResponseRefusalDoneEvent,
    ) -> Vec<GenerateContentResponse> {
        self.saw_refusal = true;
        let key = (event.output_index, event.content_index);
        let delta = compute_delta(self.refusal_buffers.get(&key), &event.refusal);
        self.refusal_buffers.insert(key, event.refusal);
        if delta.is_empty() {
            Vec::new()
        } else {
            self.emit_parts(vec![text_part(delta)])
        }
    }

    fn handle_function_call_delta(
        &mut self,
        event: ResponseFunctionCallArgumentsDeltaEvent,
    ) -> Vec<GenerateContentResponse> {
        let state = self.ensure_tool_state(
            event.output_index,
            event.item_id,
            "function".to_string(),
            ToolKind::Function,
            None,
        );
        state.arguments.push_str(&event.delta);
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn handle_function_call_done(
        &mut self,
        event: ResponseFunctionCallArgumentsDoneEvent,
    ) -> Vec<GenerateContentResponse> {
        let state = self.ensure_tool_state(
            event.output_index,
            event.item_id,
            event.name,
            ToolKind::Function,
            None,
        );
        state.arguments = event.arguments;
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn handle_mcp_call_delta(
        &mut self,
        event: ResponseMCPCallArgumentsDeltaEvent,
    ) -> Vec<GenerateContentResponse> {
        let state = self.ensure_tool_state(
            event.output_index,
            event.item_id,
            "mcp".to_string(),
            ToolKind::Mcp,
            None,
        );
        state.arguments.push_str(&event.delta);
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn handle_mcp_call_done(
        &mut self,
        event: ResponseMCPCallArgumentsDoneEvent,
    ) -> Vec<GenerateContentResponse> {
        let state = self.ensure_tool_state(
            event.output_index,
            event.item_id,
            "mcp".to_string(),
            ToolKind::Mcp,
            None,
        );
        state.arguments = event.arguments;
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn emit_function_call(
        &mut self,
        output_index: i64,
        call: FunctionToolCall,
    ) -> Vec<GenerateContentResponse> {
        let id = call.id.clone().unwrap_or_else(|| call.call_id.clone());
        let state = self.ensure_tool_state(
            output_index,
            id,
            call.name.clone(),
            ToolKind::Function,
            None,
        );
        state.arguments = call.arguments;
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn emit_custom_call(
        &mut self,
        output_index: i64,
        call: CustomToolCall,
    ) -> Vec<GenerateContentResponse> {
        let id = call.id.clone().unwrap_or_else(|| call.call_id.clone());
        let state =
            self.ensure_tool_state(output_index, id, call.name.clone(), ToolKind::Custom, None);
        state.arguments = call.input;
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn emit_mcp_call(
        &mut self,
        output_index: i64,
        call: MCPToolCall,
    ) -> Vec<GenerateContentResponse> {
        let name = format!("mcp:{}:{}", call.server_label, call.name);
        let state = self.ensure_tool_state(
            output_index,
            call.id.clone(),
            name,
            ToolKind::Mcp,
            Some(call.server_label),
        );
        state.arguments = call.arguments;
        let snapshot = state.clone();
        self.emit_tool_state(&snapshot)
    }

    fn ensure_tool_state(
        &mut self,
        output_index: i64,
        id: String,
        name: String,
        kind: ToolKind,
        server_label: Option<String>,
    ) -> &mut ToolState {
        let state = self
            .tool_states
            .entry(output_index)
            .or_insert_with(|| ToolState {
                id,
                name: name.clone(),
                kind,
                arguments: String::new(),
                server_label: server_label.clone(),
            });
        if state.name.is_empty() {
            state.name = name;
        }
        if state.server_label.is_none() {
            state.server_label = server_label;
        }
        state
    }

    fn emit_tool_state(&self, state: &ToolState) -> Vec<GenerateContentResponse> {
        let args_value = parse_json_value(&state.arguments);
        let args = match state.kind {
            ToolKind::Mcp => {
                let mut map = serde_json::Map::new();
                if let Some(server_label) = &state.server_label {
                    map.insert(
                        "server_name".to_string(),
                        JsonValue::String(server_label.clone()),
                    );
                }
                map.insert("input".to_string(), args_value);
                JsonValue::Object(map)
            }
            _ => args_value,
        };

        let part = GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(GeminiFunctionCall {
                id: Some(state.id.clone()),
                name: state.name.clone(),
                args: Some(args),
            }),
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        };

        self.emit_parts(vec![part])
    }

    fn emit_parts(&self, parts: Vec<GeminiPart>) -> Vec<GenerateContentResponse> {
        let parts: Vec<GeminiPart> = parts.into_iter().filter(part_has_payload).collect();
        if parts.is_empty() {
            return Vec::new();
        }

        let candidate = Candidate {
            content: GeminiContent {
                parts,
                role: Some(GeminiContentRole::Model),
            },
            finish_reason: None,
            safety_ratings: None,
            citation_metadata: None,
            token_count: None,
            grounding_attributions: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
        };

        vec![GenerateContentResponse {
            candidates: vec![candidate],
            prompt_feedback: None,
            usage_metadata: None,
            model_version: Some(self.model_version.clone()),
            response_id: Some(self.response_id.clone()),
            model_status: None,
        }]
    }

    fn finish_from_response(
        &mut self,
        event: ResponseCompletedEvent,
    ) -> Vec<GenerateContentResponse> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;

        self.update_from_response(&event.response);
        let finish_reason = if self.saw_refusal {
            FinishReason::Safety
        } else {
            map_finish_reason(
                event.response.status,
                event.response.incomplete_details.as_ref(),
            )
        };

        let candidate = Candidate {
            content: GeminiContent {
                parts: Vec::new(),
                role: Some(GeminiContentRole::Model),
            },
            finish_reason: Some(finish_reason),
            safety_ratings: None,
            citation_metadata: None,
            token_count: None,
            grounding_attributions: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
        };

        vec![GenerateContentResponse {
            candidates: vec![candidate],
            prompt_feedback: None,
            usage_metadata: self.usage.as_ref().map(map_usage),
            model_version: Some(self.model_version.clone()),
            response_id: Some(self.response_id.clone()),
            model_status: None,
        }]
    }

    fn update_from_response(&mut self, response: &Response) {
        self.response_id = response.id.clone();
        self.model_version = map_model_version(&response.model);
        if let Some(usage) = &response.usage {
            self.usage = Some(usage.clone());
        }
    }
}

impl Default for OpenAIResponseToGeminiStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn text_part(text: String) -> GeminiPart {
    GeminiPart {
        text: Some(text),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn compute_delta(previous: Option<&String>, full: &str) -> String {
    match previous {
        Some(prev) if full.starts_with(prev) => full[prev.len()..].to_string(),
        _ => full.to_string(),
    }
}

fn parse_json_value(raw: &str) -> JsonValue {
    serde_json::from_str(raw).unwrap_or_else(|_| JsonValue::String(raw.to_string()))
}

fn part_has_payload(part: &GeminiPart) -> bool {
    part.text
        .as_ref()
        .map(|text| !text.is_empty())
        .unwrap_or(false)
        || part.function_call.is_some()
        || part.function_response.is_some()
        || part.inline_data.is_some()
        || part.file_data.is_some()
        || part.executable_code.is_some()
        || part.code_execution_result.is_some()
        || part.thought.is_some()
        || part.thought_signature.is_some()
        || part.part_metadata.is_some()
        || part.video_metadata.is_some()
}

fn map_finish_reason(
    status: Option<ResponseStatus>,
    details: Option<&ResponseIncompleteDetails>,
) -> FinishReason {
    match status {
        Some(ResponseStatus::Incomplete) => match details.map(|d| d.reason) {
            Some(ResponseIncompleteReason::MaxOutputTokens) => FinishReason::MaxTokens,
            Some(ResponseIncompleteReason::ContentFilter) => FinishReason::Safety,
            None => FinishReason::Other,
        },
        Some(ResponseStatus::Failed) | Some(ResponseStatus::Cancelled) => FinishReason::Other,
        Some(ResponseStatus::Completed) => FinishReason::Stop,
        _ => FinishReason::Stop,
    }
}

fn map_usage(usage: &ResponseUsage) -> UsageMetadata {
    UsageMetadata {
        prompt_token_count: Some(usage.input_tokens.max(0) as u32),
        cached_content_token_count: Some(usage.input_tokens_details.cached_tokens.max(0) as u32),
        candidates_token_count: Some(usage.output_tokens.max(0) as u32),
        tool_use_prompt_token_count: None,
        thoughts_token_count: None,
        total_token_count: Some(usage.total_tokens.max(0) as u32),
        prompt_tokens_details: None,
        cache_tokens_details: None,
        candidates_tokens_details: None,
        tool_use_prompt_tokens_details: None,
    }
}

fn map_model_version(model: &str) -> String {
    if model.starts_with("models/") {
        model.to_string()
    } else {
        format!("models/{}", model)
    }
}
