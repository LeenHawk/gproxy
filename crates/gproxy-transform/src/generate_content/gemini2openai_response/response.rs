use gproxy_protocol::gemini::count_tokens::types::{Content as GeminiContent, Part as GeminiPart};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use gproxy_protocol::openai::create_response::response::Response;
use gproxy_protocol::openai::create_response::types::{
    CustomToolCall, FunctionToolCall, OutputItem, OutputMessageContent, ResponseIncompleteReason,
    ResponseStatus, ResponseUsage,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI responses response into a Gemini generate-content response.
pub fn transform_response(response: Response) -> GeminiGenerateContentResponse {
    let mut parts = Vec::new();

    for item in &response.output {
        parts.extend(map_output_item(item));
    }

    if parts.is_empty()
        && let Some(output_text) = &response.output_text
        && !output_text.is_empty()
    {
        parts.push(text_part(output_text.clone()));
    }

    let finish_reason = map_finish_reason(&response);

    let candidate = Candidate {
        content: GeminiContent {
            parts,
            role: Some(gproxy_protocol::gemini::count_tokens::types::ContentRole::Model),
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
        index: Some(0),
        finish_message: None,
    };

    GeminiGenerateContentResponse {
        candidates: vec![candidate],
        prompt_feedback: None,
        usage_metadata: response.usage.as_ref().map(map_usage),
        model_version: Some(map_model_version(&response.model)),
        response_id: Some(response.id),
        model_status: None,
    }
}

fn map_output_item(item: &OutputItem) -> Vec<GeminiPart> {
    match item {
        OutputItem::Message(message) => map_message_parts(message.content.as_slice()),
        OutputItem::Function(function) => vec![map_function_call(function)],
        OutputItem::CustomToolCall(custom) => vec![map_custom_call(custom)],
        _ => serialize_item(item),
    }
}

fn map_message_parts(contents: &[OutputMessageContent]) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for content in contents {
        match content {
            OutputMessageContent::OutputText(text) => {
                if !text.text.is_empty() {
                    parts.push(text_part(text.text.clone()));
                }
            }
            OutputMessageContent::Refusal(refusal) => {
                if !refusal.refusal.is_empty() {
                    parts.push(text_part(refusal.refusal.clone()));
                }
            }
        }
    }
    parts
}

fn map_function_call(call: &FunctionToolCall) -> GeminiPart {
    let args = serde_json::from_str(&call.arguments)
        .unwrap_or_else(|_| JsonValue::String(call.arguments.clone()));

    GeminiPart {
        text: None,
        inline_data: None,
        function_call: Some(gproxy_protocol::gemini::count_tokens::types::FunctionCall {
            id: call.id.clone().or_else(|| Some(call.call_id.clone())),
            name: call.name.clone(),
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

fn map_custom_call(call: &CustomToolCall) -> GeminiPart {
    GeminiPart {
        text: None,
        inline_data: None,
        function_call: Some(gproxy_protocol::gemini::count_tokens::types::FunctionCall {
            id: call.id.clone().or_else(|| Some(call.call_id.clone())),
            name: call.name.clone(),
            args: Some(JsonValue::String(call.input.clone())),
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

fn serialize_item(item: &OutputItem) -> Vec<GeminiPart> {
    let text = serde_json::to_string(item).unwrap_or_default();
    if text.is_empty() {
        Vec::new()
    } else {
        vec![text_part(text)]
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

fn map_finish_reason(response: &Response) -> Option<FinishReason> {
    if let Some(details) = &response.incomplete_details {
        return Some(match details.reason {
            ResponseIncompleteReason::MaxOutputTokens => FinishReason::MaxTokens,
            ResponseIncompleteReason::ContentFilter => FinishReason::Safety,
        });
    }

    match response.status {
        Some(ResponseStatus::Failed) => Some(FinishReason::Other),
        _ => Some(FinishReason::Stop),
    }
}

fn map_usage(usage: &ResponseUsage) -> UsageMetadata {
    UsageMetadata {
        prompt_token_count: Some(usage.input_tokens as u32),
        cached_content_token_count: Some(usage.input_tokens_details.cached_tokens as u32),
        candidates_token_count: Some(usage.output_tokens as u32),
        tool_use_prompt_token_count: None,
        thoughts_token_count: Some(usage.output_tokens_details.reasoning_tokens as u32),
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
