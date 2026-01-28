use gproxy_protocol::claude::count_tokens::types::Model as ClaudeModel;
use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaContentBlock, BetaMcpToolUseBlock, BetaServerToolName, BetaStopReason, BetaThinkingBlock,
    BetaUsage,
};
use gproxy_protocol::gemini::count_tokens::types::{
    Content as GeminiContent, ContentRole as GeminiContentRole, FunctionCall as GeminiFunctionCall,
    Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::response::GenerateContentResponse as GeminiGenerateContentResponse;
use gproxy_protocol::gemini::generate_content::types::{Candidate, FinishReason, UsageMetadata};
use serde_json::Value as JsonValue;

/// Convert a Claude create-message response into a Gemini generate-content response.
pub fn transform_response(response: ClaudeCreateMessageResponse) -> GeminiGenerateContentResponse {
    let parts = map_blocks_to_parts(&response.content);
    let content = GeminiContent {
        parts,
        role: Some(GeminiContentRole::Model),
    };

    let candidate = Candidate {
        content,
        finish_reason: map_stop_reason(response.stop_reason),
        safety_ratings: None,
        citation_metadata: None,
        token_count: Some(response.usage.output_tokens),
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
        usage_metadata: Some(map_usage(&response.usage)),
        model_version: Some(map_model_version(&response.model)),
        response_id: Some(response.id),
        model_status: None,
    }
}

fn map_blocks_to_parts(blocks: &[BetaContentBlock]) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for block in blocks {
        parts.extend(map_block_to_parts(block));
    }
    parts
}

fn map_block_to_parts(block: &BetaContentBlock) -> Vec<GeminiPart> {
    match block {
        BetaContentBlock::Text(text_block) => vec![text_part(text_block.text.clone())],
        BetaContentBlock::Thinking(thinking_block) => vec![thinking_part(thinking_block)],
        BetaContentBlock::RedactedThinking(redacted) => {
            let mut part = text_part(redacted.data.clone());
            part.thought = Some(true);
            vec![part]
        }
        BetaContentBlock::ToolUse(tool_use) => vec![function_call_part(
            tool_use.id.clone(),
            tool_use.name.clone(),
            json_object_to_value(&tool_use.input),
        )],
        BetaContentBlock::ServerToolUse(tool_use) => vec![function_call_part(
            tool_use.id.clone(),
            server_tool_name(tool_use.name),
            json_object_to_value(&tool_use.input),
        )],
        BetaContentBlock::McpToolUse(tool_use) => vec![mcp_tool_call_part(tool_use)],
        _ => serialize_block_as_text(block),
    }
}

fn thinking_part(block: &BetaThinkingBlock) -> GeminiPart {
    let mut part = text_part(block.thinking.clone());
    part.thought = Some(true);
    part.thought_signature = Some(block.signature.clone());
    part
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

fn function_call_part(id: String, name: String, args: JsonValue) -> GeminiPart {
    GeminiPart {
        text: None,
        inline_data: None,
        function_call: Some(GeminiFunctionCall {
            id: Some(id),
            name,
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

fn mcp_tool_call_part(tool_use: &BetaMcpToolUseBlock) -> GeminiPart {
    let mut args = serde_json::Map::new();
    args.insert(
        "server_name".to_string(),
        JsonValue::String(tool_use.server_name.clone()),
    );
    args.insert("input".to_string(), json_object_to_value(&tool_use.input));

    function_call_part(
        tool_use.id.clone(),
        format!("mcp:{}:{}", tool_use.server_name, tool_use.name),
        JsonValue::Object(args),
    )
}

fn serialize_block_as_text(block: &BetaContentBlock) -> Vec<GeminiPart> {
    // Gemini parts don't have direct equivalents for many Claude-only blocks.
    let text = serde_json::to_string(block).unwrap_or_default();
    if text.is_empty() {
        Vec::new()
    } else {
        vec![text_part(text)]
    }
}

fn json_object_to_value(
    value: &gproxy_protocol::claude::create_message::types::JsonObject,
) -> JsonValue {
    JsonValue::Object(value.clone().into_iter().collect())
}

fn server_tool_name(name: BetaServerToolName) -> String {
    match serde_json::to_value(name) {
        Ok(JsonValue::String(value)) => value,
        _ => "server_tool".to_string(),
    }
}

fn map_stop_reason(reason: Option<BetaStopReason>) -> Option<FinishReason> {
    let reason = reason?;
    Some(match reason {
        BetaStopReason::EndTurn | BetaStopReason::StopSequence => FinishReason::Stop,
        BetaStopReason::MaxTokens => FinishReason::MaxTokens,
        // Claude's tool_use is normal control flow; use STOP rather than an error finish reason.
        BetaStopReason::ToolUse => FinishReason::Stop,
        BetaStopReason::Refusal => FinishReason::Safety,
        BetaStopReason::PauseTurn | BetaStopReason::ModelContextWindowExceeded => {
            FinishReason::Other
        }
    })
}

fn map_usage(usage: &BetaUsage) -> UsageMetadata {
    let total = usage.input_tokens.saturating_add(usage.output_tokens);
    UsageMetadata {
        prompt_token_count: Some(usage.input_tokens),
        cached_content_token_count: if usage.cache_read_input_tokens > 0 {
            Some(usage.cache_read_input_tokens)
        } else {
            None
        },
        candidates_token_count: Some(usage.output_tokens),
        tool_use_prompt_token_count: None,
        thoughts_token_count: None,
        total_token_count: Some(total),
        prompt_tokens_details: None,
        cache_tokens_details: None,
        candidates_tokens_details: None,
        tool_use_prompt_tokens_details: None,
    }
}

fn map_model_version(model: &ClaudeModel) -> String {
    let model_id = match model {
        ClaudeModel::Custom(value) => value.clone(),
        ClaudeModel::Known(known) => match serde_json::to_value(known) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        },
    };

    if model_id.starts_with("models/") {
        model_id
    } else {
        format!("models/{}", model_id)
    }
}
