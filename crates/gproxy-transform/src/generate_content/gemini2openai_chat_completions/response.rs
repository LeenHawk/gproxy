use gproxy_protocol::gemini::count_tokens::types::{
    Content as GeminiContent, ContentRole as GeminiContentRole, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_chat_completions::response::CreateChatCompletionResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCall, ChatCompletionResponseMessage,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI chat-completions response into a Gemini generate-content response.
pub fn transform_response(response: CreateChatCompletionResponse) -> GeminiGenerateContentResponse {
    let candidates = response
        .choices
        .iter()
        .map(|choice| map_choice_to_candidate(choice, &response.model))
        .collect::<Vec<Candidate>>();

    GeminiGenerateContentResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata: response.usage.as_ref().map(map_usage),
        model_version: Some(map_model_version(&response.model)),
        response_id: Some(response.id),
        model_status: None,
    }
}

fn map_choice_to_candidate(
    choice: &gproxy_protocol::openai::create_chat_completions::response::ChatCompletionChoice,
    model: &str,
) -> Candidate {
    let content = map_message_to_content(&choice.message, model);
    Candidate {
        content,
        finish_reason: Some(map_finish_reason(choice.finish_reason)),
        safety_ratings: None,
        citation_metadata: None,
        token_count: None,
        grounding_attributions: None,
        grounding_metadata: None,
        avg_logprobs: None,
        logprobs_result: None,
        url_context_metadata: None,
        index: if choice.index >= 0 {
            Some(choice.index as u32)
        } else {
            None
        },
        finish_message: None,
    }
}

fn map_message_to_content(message: &ChatCompletionResponseMessage, _model: &str) -> GeminiContent {
    let mut parts = Vec::new();

    if let Some(text) = &message.content
        && !text.is_empty()
    {
        parts.push(text_part(text.clone()));
    }

    if let Some(refusal) = &message.refusal
        && !refusal.is_empty()
    {
        parts.push(text_part(refusal.clone()));
    }

    if let Some(tool_calls) = &message.tool_calls {
        for call in tool_calls {
            parts.push(map_tool_call(call));
        }
    }

    if let Some(function_call) = &message.function_call {
        let args = serde_json::from_str(&function_call.arguments)
            .unwrap_or_else(|_| JsonValue::String(function_call.arguments.clone()));
        parts.push(GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(gproxy_protocol::gemini::count_tokens::types::FunctionCall {
                id: None,
                name: function_call.name.clone(),
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
        });
    }

    GeminiContent {
        parts,
        role: Some(GeminiContentRole::Model),
    }
}

fn map_tool_call(call: &ChatCompletionMessageToolCall) -> GeminiPart {
    match call {
        ChatCompletionMessageToolCall::Function { id, function } => {
            let args = serde_json::from_str(&function.arguments)
                .unwrap_or_else(|_| JsonValue::String(function.arguments.clone()));
            GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(gproxy_protocol::gemini::count_tokens::types::FunctionCall {
                    id: Some(id.clone()),
                    name: function.name.clone(),
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
            }
        }
        ChatCompletionMessageToolCall::Custom { id, custom } => GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(gproxy_protocol::gemini::count_tokens::types::FunctionCall {
                id: Some(id.clone()),
                name: custom.name.clone(),
                args: Some(JsonValue::String(custom.input.clone())),
            }),
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        },
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
    UsageMetadata {
        prompt_token_count: Some(usage.prompt_tokens as u32),
        cached_content_token_count: usage
            .prompt_tokens_details
            .as_ref()
            .and_then(|details| details.cached_tokens.map(|value| value as u32)),
        candidates_token_count: Some(usage.completion_tokens as u32),
        tool_use_prompt_token_count: None,
        thoughts_token_count: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens.map(|value| value as u32)),
        total_token_count: Some(usage.total_tokens as u32),
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
