use gproxy_protocol::gemini::count_tokens::types::Content as GeminiContent;
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_chat_completions::response::{
    ChatCompletionChoice, ChatCompletionObjectType, CreateChatCompletionResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCall, ChatCompletionResponseMessage,
    ChatCompletionResponseRole, CompletionTokensDetails, CompletionUsage, PromptTokensDetails,
};

/// Convert a Gemini generate-content response into an OpenAI chat-completions response.
pub fn transform_response(response: GeminiGenerateContentResponse) -> CreateChatCompletionResponse {
    let model = map_model_name(
        response
            .model_version
            .clone()
            .or_else(|| {
                response
                    .model_status
                    .as_ref()
                    .map(|status| format!("{:?}", status.model_stage))
            })
            .unwrap_or_else(|| "unknown".to_string()),
    );

    let choices = if response.candidates.is_empty() {
        vec![ChatCompletionChoice {
            index: 0,
            message: ChatCompletionResponseMessage {
                role: ChatCompletionResponseRole::Assistant,
                content: None,
                refusal: None,
                tool_calls: None,
                annotations: None,
                function_call: None,
                audio: None,
            },
            finish_reason: ChatCompletionFinishReason::Stop,
            logprobs: None,
        }]
    } else {
        response
            .candidates
            .iter()
            .enumerate()
            .map(|(idx, candidate)| map_candidate_to_choice(candidate, idx))
            .collect()
    };

    CreateChatCompletionResponse {
        id: response
            .response_id
            .unwrap_or_else(|| "response".to_string()),
        object: ChatCompletionObjectType::ChatCompletion,
        created: 0,
        model,
        choices,
        usage: response.usage_metadata.as_ref().map(map_usage),
        service_tier: None,
        system_fingerprint: None,
    }
}

fn map_candidate_to_choice(candidate: &Candidate, fallback_index: usize) -> ChatCompletionChoice {
    let (content, tool_calls) = map_content_to_message_parts(&candidate.content, fallback_index);
    let message = ChatCompletionResponseMessage {
        role: ChatCompletionResponseRole::Assistant,
        content,
        refusal: None,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        annotations: None,
        function_call: None,
        audio: None,
    };

    ChatCompletionChoice {
        index: candidate.index.unwrap_or(fallback_index as u32) as i64,
        message,
        finish_reason: candidate
            .finish_reason
            .map(map_finish_reason)
            .unwrap_or(ChatCompletionFinishReason::Stop),
        logprobs: None,
    }
}

fn map_content_to_message_parts(
    content: &GeminiContent,
    index: usize,
) -> (Option<String>, Vec<ChatCompletionMessageToolCall>) {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_call_counter = 0usize;

    for part in &content.parts {
        if let Some(text) = part.text.clone()
            && !text.is_empty()
        {
            texts.push(text);
        }

        if let Some(function_call) = &part.function_call {
            let id = function_call
                .id
                .clone()
                .unwrap_or_else(|| format!("tool_call_{}_{}", index, tool_call_counter));
            tool_call_counter += 1;

            let args = function_call
                .args
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_else(|| "{}".to_string());
            tool_calls.push(ChatCompletionMessageToolCall::Function {
                id,
                function: gproxy_protocol::openai::create_chat_completions::types::ChatCompletionMessageToolCallFunction {
                    name: function_call.name.clone(),
                    arguments: args,
                },
            });
        }

        if let Some(function_response) = &part.function_response
            && let Ok(text) = serde_json::to_string(function_response)
            && !text.is_empty()
        {
            texts.push(text);
        }

        if let Some(code) = &part.executable_code
            && let Ok(text) = serde_json::to_string(code)
            && !text.is_empty()
        {
            texts.push(text);
        }

        if let Some(result) = &part.code_execution_result
            && let Ok(text) = serde_json::to_string(result)
            && !text.is_empty()
        {
            texts.push(text);
        }

        if part.inline_data.is_some() {
            texts.push("[inline_data]".to_string());
        }

        if let Some(file_data) = &part.file_data {
            texts.push(format!("[file:{}]", file_data.file_uri));
        }
    }

    let content = if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    };

    (content, tool_calls)
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
