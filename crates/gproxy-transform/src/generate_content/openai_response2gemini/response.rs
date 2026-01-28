use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::types::{
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, OutputItem, OutputMessage,
    OutputMessageContent, OutputMessageRole, OutputMessageType, ResponseIncompleteDetails,
    ResponseIncompleteReason, ResponseStatus, ResponseUsage, ResponseUsageInputTokensDetails,
    ResponseUsageOutputTokensDetails,
};

/// Convert a Gemini generate-content response into an OpenAI responses response.
pub fn transform_response(response: GeminiGenerateContentResponse) -> Response {
    let mut output = Vec::new();

    for (index, candidate) in response.candidates.iter().enumerate() {
        output.extend(map_candidate_to_output(candidate, index));
    }

    let usage = response.usage_metadata.as_ref().map(map_usage);
    let output_text = extract_output_text(&output);
    let (status, incomplete_details) = map_status(&response);

    let model = response
        .model_version
        .or_else(|| {
            response
                .model_status
                .map(|status| format!("{:?}", status.model_stage))
        })
        .unwrap_or_else(|| "unknown".to_string());
    let model = model.strip_prefix("models/").unwrap_or(&model).to_string();

    Response {
        id: response
            .response_id
            .unwrap_or_else(|| "response".to_string()),
        object: ResponseObjectType::Response,
        created_at: 0,
        status: Some(status),
        completed_at: None,
        error: None,
        incomplete_details,
        instructions: None,
        model,
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

fn map_candidate_to_output(candidate: &Candidate, index: usize) -> Vec<OutputItem> {
    let mut items = Vec::new();
    let (message, tool_calls) = map_candidate_message(candidate, index);

    if let Some(message) = message {
        items.push(OutputItem::Message(message));
    }

    for tool_call in tool_calls {
        items.push(OutputItem::Function(tool_call));
    }

    items
}

fn map_candidate_message(
    candidate: &Candidate,
    index: usize,
) -> (Option<OutputMessage>, Vec<FunctionToolCall>) {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();
    let mut tool_call_counter = 0usize;

    for part in &candidate.content.parts {
        if let Some(text) = part.text.clone()
            && !text.is_empty()
        {
            texts.push(text);
        }

        if let Some(function_call) = &part.function_call {
            let call_id = function_call
                .id
                .clone()
                .unwrap_or_else(|| format!("tool_call_{}_{}", index, tool_call_counter));
            tool_call_counter += 1;
            let args = function_call
                .args
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_else(|| "{}".to_string());
            tool_calls.push(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(call_id.clone()),
                call_id,
                name: function_call.name.clone(),
                arguments: args,
                status: Some(FunctionCallItemStatus::Completed),
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

    let message = if texts.is_empty() {
        None
    } else {
        Some(OutputMessage {
            id: format!("message_{index}"),
            r#type: OutputMessageType::Message,
            role: OutputMessageRole::Assistant,
            content: vec![OutputMessageContent::OutputText(
                gproxy_protocol::openai::create_response::types::OutputTextContent {
                    text: texts.join("\n"),
                    annotations: Vec::new(),
                    logprobs: None,
                },
            )],
            status: gproxy_protocol::openai::create_response::types::MessageStatus::Completed,
        })
    };

    (message, tool_calls)
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

fn map_status(
    response: &GeminiGenerateContentResponse,
) -> (ResponseStatus, Option<ResponseIncompleteDetails>) {
    let finish_reason = response
        .candidates
        .first()
        .and_then(|candidate| candidate.finish_reason);

    match finish_reason {
        Some(FinishReason::MaxTokens) => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::MaxOutputTokens,
            }),
        ),
        Some(
            FinishReason::Safety
            | FinishReason::Blocklist
            | FinishReason::ProhibitedContent
            | FinishReason::Spii
            | FinishReason::ImageSafety
            | FinishReason::ImageProhibitedContent
            | FinishReason::ImageRecitation
            | FinishReason::NoImage
            | FinishReason::Recitation,
        ) => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::ContentFilter,
            }),
        ),
        _ => (ResponseStatus::Completed, None),
    }
}
