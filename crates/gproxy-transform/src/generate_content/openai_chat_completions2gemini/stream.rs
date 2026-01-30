use std::collections::BTreeMap;

use gproxy_protocol::gemini::count_tokens::types::{
    Content as GeminiContent, ContentRole as GeminiContentRole, FunctionCall as GeminiFunctionCall,
    Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_chat_completions::stream::CreateChatCompletionStreamResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionFunctionCallDelta, ChatCompletionMessageToolCallChunk,
};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone)]
struct ToolCallState {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone)]
pub struct OpenAIChatCompletionToGeminiStreamState {
    response_id: String,
    model_version: String,
    content_parts: BTreeMap<i64, Vec<GeminiPart>>,
    tool_calls: BTreeMap<(i64, i64), ToolCallState>,
    usage: Option<UsageMetadata>,
    pending_finishes: BTreeMap<i64, FinishReason>,
}

impl OpenAIChatCompletionToGeminiStreamState {
    pub fn new() -> Self {
        Self {
            response_id: "response".to_string(),
            model_version: "models/unknown".to_string(),
            content_parts: BTreeMap::new(),
            tool_calls: BTreeMap::new(),
            usage: None,
            pending_finishes: BTreeMap::new(),
        }
    }

    pub fn transform_event(
        &mut self,
        chunk: CreateChatCompletionStreamResponse,
    ) -> Vec<GenerateContentResponse> {
        self.update_from_chunk(&chunk);

        let mut responses = Vec::new();
        if let Some(usage) = &chunk.usage {
            self.usage = Some(map_usage(usage));
        }

        let mut finish_reasons = Vec::new();
        for choice in chunk.choices {
            let choice_index = choice.index;
            let delta = choice.delta;

            if let Some(content) = delta.content {
                responses.extend(self.emit_text(choice_index, content));
            } else if let Some(reasoning) = delta.reasoning_content {
                responses.extend(self.emit_text(choice_index, reasoning));
            }

            if let Some(refusal) = delta.refusal {
                responses.extend(self.emit_text(choice_index, refusal));
            }

            if let Some(function_call) = delta.function_call {
                responses.extend(self.handle_function_call(choice_index, function_call));
            }

            if let Some(tool_calls) = delta.tool_calls {
                for tool_call in tool_calls {
                    responses.extend(self.handle_tool_call(choice_index, tool_call));
                }
            }

            if let Some(reason) = choice.finish_reason {
                finish_reasons.push((choice_index, reason));
            }
        }

        for (choice_index, reason) in finish_reasons {
            let finish_reason = map_finish_reason(reason);
            if self.usage.is_some() {
                responses.push(self.finish_choice(choice_index, finish_reason));
            } else {
                self.pending_finishes.insert(choice_index, finish_reason);
            }
        }

        if self.usage.is_some() && !self.pending_finishes.is_empty() {
            let pending = std::mem::take(&mut self.pending_finishes);
            for (choice_index, reason) in pending {
                responses.push(self.finish_choice(choice_index, reason));
            }
        }

        responses
    }

    fn emit_text(&mut self, choice_index: i64, text: String) -> Vec<GenerateContentResponse> {
        if text.is_empty() {
            return Vec::new();
        }

        vec![self.build_response(choice_index, vec![text_part(text)], None)]
    }

    fn handle_function_call(
        &mut self,
        choice_index: i64,
        call: ChatCompletionFunctionCallDelta,
    ) -> Vec<GenerateContentResponse> {
        let name = call.name.unwrap_or_else(|| "function_call".to_string());
        let arguments = call.arguments.unwrap_or_default();
        self.emit_tool_delta(choice_index, -1, None, name, arguments)
    }

    fn handle_tool_call(
        &mut self,
        choice_index: i64,
        call: ChatCompletionMessageToolCallChunk,
    ) -> Vec<GenerateContentResponse> {
        let name = call
            .function
            .as_ref()
            .and_then(|function| function.name.clone())
            .unwrap_or_else(|| "tool_call".to_string());
        let arguments = call
            .function
            .as_ref()
            .and_then(|function| function.arguments.clone())
            .unwrap_or_default();
        self.emit_tool_delta(choice_index, call.index, call.id, name, arguments)
    }

    fn emit_tool_delta(
        &mut self,
        choice_index: i64,
        tool_index: i64,
        id: Option<String>,
        name: String,
        arguments: String,
    ) -> Vec<GenerateContentResponse> {
        let key = (choice_index, tool_index);
        let state = self.tool_calls.entry(key).or_insert_with(|| ToolCallState {
            name: name.clone(),
            arguments: String::new(),
        });

        if state.name.is_empty() {
            state.name = name.clone();
        }
        if !arguments.is_empty() {
            state.arguments.push_str(&arguments);
        }

        let args_value = serde_json::from_str(&state.arguments)
            .unwrap_or_else(|_| JsonValue::String(state.arguments.clone()));
        let part = GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(GeminiFunctionCall {
                id: id.clone(),
                name: state.name.clone(),
                args: Some(args_value),
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

        let parts = self.content_parts.entry(choice_index).or_default();
        parts.push(part);
        let parts_snapshot = parts.clone();
        vec![self.build_response(choice_index, parts_snapshot, None)]
    }

    fn finish_choice(&mut self, choice_index: i64, finish_reason: FinishReason) -> GenerateContentResponse {
        let parts = self.content_parts.remove(&choice_index).unwrap_or_default();
        self.build_response(choice_index, parts, Some(finish_reason))
    }

    fn build_response(
        &self,
        choice_index: i64,
        parts: Vec<GeminiPart>,
        finish_reason: Option<FinishReason>,
    ) -> GenerateContentResponse {
        GenerateContentResponse {
            candidates: vec![Candidate {
                content: GeminiContent {
                    parts,
                    role: Some(GeminiContentRole::Model),
                },
                finish_reason,
                safety_ratings: None,
                citation_metadata: None,
                token_count: None,
                grounding_attributions: None,
                grounding_metadata: None,
                avg_logprobs: None,
                logprobs_result: None,
                url_context_metadata: None,
                index: Some(choice_index as u32),
                finish_message: None,
            }],
            prompt_feedback: None,
            usage_metadata: finish_reason.and_then(|_| self.usage.clone()),
            model_version: Some(self.model_version.clone()),
            response_id: Some(self.response_id.clone()),
            model_status: None,
        }
    }


    fn update_from_chunk(&mut self, chunk: &CreateChatCompletionStreamResponse) {
        self.response_id = chunk.id.clone();
        self.model_version = map_model_version(&chunk.model);
    }
}

impl Default for OpenAIChatCompletionToGeminiStreamState {
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


fn map_finish_reason(reason: ChatCompletionFinishReason) -> FinishReason {
    match reason {
        ChatCompletionFinishReason::Stop => FinishReason::Stop,
        ChatCompletionFinishReason::Length => FinishReason::MaxTokens,
        ChatCompletionFinishReason::ToolCalls | ChatCompletionFinishReason::FunctionCall => {
            FinishReason::UnexpectedToolCall
        }
        ChatCompletionFinishReason::ContentFilter => FinishReason::Safety,
    }
}

fn map_usage(
    usage: &gproxy_protocol::openai::create_chat_completions::types::CompletionUsage,
) -> UsageMetadata {
    let prompt_tokens = usage.prompt_tokens as u32;
    let completion_tokens = usage.completion_tokens as u32;
    let total_tokens = usage.total_tokens as u32;

    UsageMetadata {
        prompt_token_count: Some(prompt_tokens),
        cached_content_token_count: usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens.map(|value| value as u32)),
        candidates_token_count: Some(completion_tokens),
        tool_use_prompt_token_count: None,
        thoughts_token_count: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens.map(|value| value as u32)),
        total_token_count: Some(total_tokens),
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
