use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaCacheCreation, BetaContentBlock, BetaMessage, BetaMessageRole, BetaMessageType,
    BetaServerToolUsage, BetaServiceTierUsed, BetaStopReason, BetaTextBlock, BetaTextBlockType,
    BetaToolUseBlock, BetaToolUseBlockType, BetaUsage, JsonObject, JsonValue,
};
use gproxy_protocol::gemini::count_tokens::types::{Content as GeminiContent, Part as GeminiPart};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{FinishReason, UsageMetadata};

/// Convert a Gemini generate-content response into a Claude create-message response.
pub fn transform_response(response: GeminiGenerateContentResponse) -> ClaudeCreateMessageResponse {
    let candidate = response.candidates.first();

    let content_blocks = candidate
        .map(|candidate| map_content_to_blocks(&candidate.content))
        .unwrap_or_default();

    let stop_reason = candidate.and_then(|candidate| map_finish_reason(candidate.finish_reason));

    let usage = map_usage(response.usage_metadata);

    let model_id = response
        .model_version
        .or_else(|| {
            response
                .model_status
                .map(|status| format!("{:?}", status.model_stage))
        })
        .unwrap_or_else(|| "unknown".to_string());

    let model_id = if model_id.starts_with("models/") {
        model_id.trim_start_matches("models/").to_string()
    } else {
        model_id
    };

    BetaMessage {
        id: response
            .response_id
            .unwrap_or_else(|| "response".to_string()),
        container: None,
        content: content_blocks,
        context_management: None,
        model: gproxy_protocol::claude::count_tokens::types::Model::Custom(model_id),
        role: BetaMessageRole::Assistant,
        stop_reason,
        stop_sequence: None,
        r#type: BetaMessageType::Message,
        usage,
    }
}

fn map_content_to_blocks(content: &GeminiContent) -> Vec<BetaContentBlock> {
    let mut blocks = Vec::new();
    for part in &content.parts {
        blocks.extend(map_part_to_blocks(part));
    }
    blocks
}

fn map_part_to_blocks(part: &GeminiPart) -> Vec<BetaContentBlock> {
    let mut blocks = Vec::new();

    if let Some(text) = part.text.clone()
        && !text.is_empty()
    {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text,
            r#type: BetaTextBlockType::Text,
        }));
    }

    if let Some(function_call) = &part.function_call {
        let input = map_json_object(function_call.args.as_ref());
        blocks.push(BetaContentBlock::ToolUse(BetaToolUseBlock {
            id: function_call
                .id
                .clone()
                .unwrap_or_else(|| function_call.name.clone()),
            input,
            name: function_call.name.clone(),
            r#type: BetaToolUseBlockType::ToolUse,
            caller: None,
        }));
    }

    if let Some(function_response) = &part.function_response {
        let text = serde_json::to_string(function_response).unwrap_or_default();
        if !text.is_empty() {
            blocks.push(BetaContentBlock::Text(BetaTextBlock {
                citations: None,
                text,
                r#type: BetaTextBlockType::Text,
            }));
        }
    }

    if let Some(code) = &part.executable_code
        && let Ok(text) = serde_json::to_string(code)
    {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text,
            r#type: BetaTextBlockType::Text,
        }));
    }

    if let Some(result) = &part.code_execution_result
        && let Ok(text) = serde_json::to_string(result)
    {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text,
            r#type: BetaTextBlockType::Text,
        }));
    }

    if part.inline_data.is_some() {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text: "[inline_data]".to_string(),
            r#type: BetaTextBlockType::Text,
        }));
    }

    if let Some(file_data) = &part.file_data {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text: format!("[file:{}]", file_data.file_uri),
            r#type: BetaTextBlockType::Text,
        }));
    }

    blocks
}

fn map_json_object(value: Option<&JsonValue>) -> JsonObject {
    match value {
        Some(JsonValue::Object(map)) => map.clone().into_iter().collect(),
        Some(other) => {
            let mut map = JsonObject::new();
            map.insert("arguments".to_string(), other.clone());
            map
        }
        None => JsonObject::new(),
    }
}

fn map_finish_reason(reason: Option<FinishReason>) -> Option<BetaStopReason> {
    let reason = reason?;
    Some(match reason {
        FinishReason::Stop => BetaStopReason::EndTurn,
        FinishReason::MaxTokens => BetaStopReason::MaxTokens,
        FinishReason::MalformedFunctionCall
        | FinishReason::UnexpectedToolCall
        | FinishReason::TooManyToolCalls => BetaStopReason::ToolUse,
        FinishReason::Safety
        | FinishReason::Blocklist
        | FinishReason::ProhibitedContent
        | FinishReason::Spii
        | FinishReason::ImageSafety
        | FinishReason::ImageProhibitedContent
        | FinishReason::ImageRecitation
        | FinishReason::NoImage
        | FinishReason::Recitation => BetaStopReason::Refusal,
        _ => BetaStopReason::EndTurn,
    })
}

fn map_usage(usage: Option<UsageMetadata>) -> BetaUsage {
    let input_tokens = usage
        .as_ref()
        .and_then(|usage| usage.prompt_token_count)
        .unwrap_or(0);
    let output_tokens = usage
        .as_ref()
        .and_then(|usage| usage.candidates_token_count)
        .unwrap_or(0);

    BetaUsage {
        cache_creation: BetaCacheCreation {
            ephemeral_1h_input_tokens: 0,
            ephemeral_5m_input_tokens: 0,
        },
        cache_creation_input_tokens: 0,
        cache_read_input_tokens: 0,
        input_tokens,
        output_tokens,
        server_tool_use: Some(BetaServerToolUsage {
            web_fetch_requests: 0,
            web_search_requests: 0,
        }),
        service_tier: BetaServiceTierUsed::Standard,
    }
}
