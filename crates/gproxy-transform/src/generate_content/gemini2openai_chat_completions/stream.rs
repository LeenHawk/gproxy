use std::collections::BTreeMap;

use gproxy_protocol::gemini::count_tokens::types::Part as GeminiPart;
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_chat_completions::stream::{
    ChatCompletionChunkObjectType, ChatCompletionStreamChoice, CreateChatCompletionStreamResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCallChunk,
    ChatCompletionMessageToolCallChunkFunction, ChatCompletionRole,
    ChatCompletionStreamResponseDelta, ChatCompletionToolCallChunkType, CompletionTokensDetails,
    CompletionUsage, PromptTokensDetails,
};

#[derive(Debug, Clone)]
struct ToolCallState {
    index: i64,
    id: String,
    name: String,
    arguments: String,
    id_sent: bool,
    name_sent: bool,
}

#[derive(Debug, Clone)]
pub struct GeminiToOpenAIChatCompletionStreamState {
    id: String,
    model: String,
    created: i64,
    role_sent: BTreeMap<i64, bool>,
    text_buffers: BTreeMap<i64, String>,
    tool_calls: BTreeMap<(i64, String), ToolCallState>,
    tool_counters: BTreeMap<i64, i64>,
    usage: Option<CompletionUsage>,
}

impl GeminiToOpenAIChatCompletionStreamState {
    pub fn new() -> Self {
        Self {
            id: "response".to_string(),
            model: "unknown".to_string(),
            created: 0,
            role_sent: BTreeMap::new(),
            text_buffers: BTreeMap::new(),
            tool_calls: BTreeMap::new(),
            tool_counters: BTreeMap::new(),
            usage: None,
        }
    }

    pub fn transform_response(
        &mut self,
        response: GenerateContentResponse,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        self.update_from_response(&response);
        if let Some(usage) = &response.usage_metadata {
            self.usage = Some(map_usage(usage));
        }

        let mut events = Vec::new();
        let mut finish_reasons = Vec::new();

        for (idx, candidate) in response.candidates.iter().enumerate() {
            let choice_index = candidate
                .index
                .map(|value| value as i64)
                .unwrap_or(idx as i64);
            events.extend(self.handle_parts(choice_index, &candidate.content.parts));
            if let Some(reason) = candidate.finish_reason {
                finish_reasons.push((choice_index, reason));
            }
        }

        for (choice_index, reason) in finish_reasons {
            events.push(self.finish_choice(choice_index, reason));
        }

        events
    }

    fn handle_parts(
        &mut self,
        choice_index: i64,
        parts: &[GeminiPart],
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let mut events = Vec::new();
        for part in parts {
            events.extend(self.handle_part(choice_index, part));
        }
        events
    }

    fn handle_part(
        &mut self,
        choice_index: i64,
        part: &GeminiPart,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let mut events = Vec::new();

        if let Some(text) = &part.text
            && !text.is_empty()
        {
            events.push(self.emit_text_delta(choice_index, text.clone()));
        }

        if let Some(function_call) = &part.function_call {
            let args = function_call
                .args
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_else(|| "{}".to_string());
            let call_id = function_call
                .id
                .clone()
                .unwrap_or_else(|| self.next_tool_id(choice_index));
            events.extend(self.emit_tool_delta(
                choice_index,
                call_id,
                function_call.name.clone(),
                args,
            ));
        }

        if let Some(function_response) = &part.function_response
            && let Ok(text) = serde_json::to_string(function_response)
            && !text.is_empty()
        {
            events.push(self.emit_text_delta(choice_index, text));
        }

        if let Some(code) = &part.executable_code
            && let Ok(text) = serde_json::to_string(code)
            && !text.is_empty()
        {
            events.push(self.emit_text_delta(choice_index, text));
        }

        if let Some(result) = &part.code_execution_result
            && let Ok(text) = serde_json::to_string(result)
            && !text.is_empty()
        {
            events.push(self.emit_text_delta(choice_index, text));
        }

        if part.inline_data.is_some() {
            events.push(self.emit_text_delta(choice_index, "[inline_data]".to_string()));
        }

        if let Some(file_data) = &part.file_data {
            events
                .push(self.emit_text_delta(choice_index, format!("[file:{}]", file_data.file_uri)));
        }

        events
    }

    fn emit_text_delta(
        &mut self,
        choice_index: i64,
        text: String,
    ) -> CreateChatCompletionStreamResponse {
        let delta_text = {
            let buffer = self.text_buffers.entry(choice_index).or_default();
            if buffer.is_empty() {
                buffer.push_str(&text);
                text
            } else {
                buffer.push('\n');
                buffer.push_str(&text);
                format!("\n{}", text)
            }
        };

        let role = self.take_role(choice_index);
        self.make_chunk(
            choice_index,
            ChatCompletionStreamResponseDelta {
                content: Some(delta_text),
                reasoning_content: None,
                function_call: None,
                tool_calls: None,
                role,
                refusal: None,
                obfuscation: None,
            },
            None,
        )
    }

    fn emit_tool_delta(
        &mut self,
        choice_index: i64,
        call_id: String,
        name: String,
        arguments: String,
    ) -> Vec<CreateChatCompletionStreamResponse> {
        let key = (choice_index, call_id.clone());
        let tool_state = if let Some(state) = self.tool_calls.get_mut(&key) {
            state
        } else {
            let index = self.next_tool_index(choice_index);
            let state = ToolCallState {
                index,
                id: call_id.clone(),
                name: name.clone(),
                arguments: String::new(),
                id_sent: false,
                name_sent: false,
            };
            self.tool_calls.insert(key.clone(), state);
            self.tool_calls.get_mut(&key).expect("tool state")
        };

        if tool_state.name.is_empty() {
            tool_state.name = name.clone();
        }

        let delta = compute_delta(Some(&tool_state.arguments), &arguments);
        tool_state.arguments = arguments;
        let id_to_send = if tool_state.id_sent {
            None
        } else {
            tool_state.id_sent = true;
            Some(tool_state.id.clone())
        };
        let name_to_send = if tool_state.name_sent {
            None
        } else {
            tool_state.name_sent = true;
            Some(tool_state.name.clone())
        };
        let args_to_send = if delta.is_empty() { None } else { Some(delta) };

        if id_to_send.is_none() && name_to_send.is_none() && args_to_send.is_none() {
            return Vec::new();
        }

        let chunk = ChatCompletionMessageToolCallChunk {
            index: tool_state.index,
            id: id_to_send,
            r#type: Some(ChatCompletionToolCallChunkType::Function),
            function: Some(ChatCompletionMessageToolCallChunkFunction {
                name: name_to_send,
                arguments: args_to_send,
            }),
        };

        let role = self.take_role(choice_index);
        vec![self.make_chunk(
            choice_index,
            ChatCompletionStreamResponseDelta {
                content: None,
                reasoning_content: None,
                function_call: None,
                tool_calls: Some(vec![chunk]),
                role,
                refusal: None,
                obfuscation: None,
            },
            None,
        )]
    }

    fn finish_choice(
        &mut self,
        choice_index: i64,
        reason: FinishReason,
    ) -> CreateChatCompletionStreamResponse {
        let finish_reason = map_finish_reason(reason);
        let role = if self.role_sent.get(&choice_index).copied().unwrap_or(false) {
            None
        } else {
            Some(ChatCompletionRole::Assistant)
        };

        self.make_chunk(
            choice_index,
            ChatCompletionStreamResponseDelta {
                content: None,
                reasoning_content: None,
                function_call: None,
                tool_calls: None,
                role,
                refusal: None,
                obfuscation: None,
            },
            Some(finish_reason),
        )
    }

    fn make_chunk(
        &self,
        choice_index: i64,
        delta: ChatCompletionStreamResponseDelta,
        finish_reason: Option<ChatCompletionFinishReason>,
    ) -> CreateChatCompletionStreamResponse {
        CreateChatCompletionStreamResponse {
            id: self.id.clone(),
            object: ChatCompletionChunkObjectType::ChatCompletionChunk,
            created: self.created,
            model: self.model.clone(),
            choices: vec![ChatCompletionStreamChoice {
                index: choice_index,
                delta,
                logprobs: None,
                finish_reason,
            }],
            usage: if finish_reason.is_some() {
                self.usage.clone()
            } else {
                None
            },
            service_tier: None,
            system_fingerprint: None,
        }
    }

    fn take_role(&mut self, choice_index: i64) -> Option<ChatCompletionRole> {
        if self.role_sent.get(&choice_index).copied().unwrap_or(false) {
            None
        } else {
            self.role_sent.insert(choice_index, true);
            Some(ChatCompletionRole::Assistant)
        }
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
            self.model = map_model_name(model);
        }
    }

    fn next_tool_id(&mut self, choice_index: i64) -> String {
        let counter = self.next_tool_index(choice_index);
        format!("tool_call_{}_{}", choice_index, counter)
    }

    fn next_tool_index(&mut self, choice_index: i64) -> i64 {
        let counter = self.tool_counters.entry(choice_index).or_insert(0);
        let index = *counter;
        *counter += 1;
        index
    }
}

impl Default for GeminiToOpenAIChatCompletionStreamState {
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

fn map_finish_reason(reason: FinishReason) -> ChatCompletionFinishReason {
    match reason {
        FinishReason::Stop => ChatCompletionFinishReason::Stop,
        FinishReason::MaxTokens => ChatCompletionFinishReason::Length,
        FinishReason::MalformedFunctionCall
        | FinishReason::UnexpectedToolCall
        | FinishReason::TooManyToolCalls => ChatCompletionFinishReason::ToolCalls,
        FinishReason::Safety
        | FinishReason::Blocklist
        | FinishReason::ProhibitedContent
        | FinishReason::Spii
        | FinishReason::ImageSafety
        | FinishReason::ImageProhibitedContent
        | FinishReason::ImageRecitation
        | FinishReason::NoImage
        | FinishReason::Recitation => ChatCompletionFinishReason::ContentFilter,
        _ => ChatCompletionFinishReason::Stop,
    }
}

fn map_usage(usage: &UsageMetadata) -> CompletionUsage {
    let prompt_tokens = usage.prompt_token_count.unwrap_or(0) as i64;
    let completion_tokens = usage.candidates_token_count.unwrap_or(0) as i64;
    let total_tokens = usage
        .total_token_count
        .map(|value| value as i64)
        .unwrap_or_else(|| prompt_tokens + completion_tokens);

    CompletionUsage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        completion_tokens_details: Some(CompletionTokensDetails {
            accepted_prediction_tokens: None,
            audio_tokens: None,
            reasoning_tokens: usage.thoughts_token_count.map(|value| value as i64),
            rejected_prediction_tokens: None,
        }),
        prompt_tokens_details: Some(PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: usage.cached_content_token_count.map(|value| value as i64),
        }),
    }
}

fn map_model_name(model: String) -> String {
    model.strip_prefix("models/").unwrap_or(&model).to_string()
}
