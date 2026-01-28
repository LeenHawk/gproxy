use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaContentBlock, BetaMcpToolUseBlock, BetaMessage, BetaServerToolUseBlock, BetaStopReason,
    BetaToolUseBlock,
};
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::types::{
    FunctionCallItemStatus, FunctionToolCall, FunctionToolCallType, MCPToolCall, MCPToolCallStatus,
    MCPToolCallType, MessageStatus, OutputItem, OutputMessage, OutputMessageContent,
    OutputMessageRole, OutputMessageType, OutputTextContent, RefusalContent,
    ResponseIncompleteDetails, ResponseIncompleteReason, ResponseStatus, ResponseUsage,
    ResponseUsageInputTokensDetails, ResponseUsageOutputTokensDetails,
};
use serde_json::Value as JsonValue;

/// Convert a Claude create-message response into an OpenAI responses object.
pub fn transform_response(response: ClaudeCreateMessageResponse) -> Response {
    let (output, output_text) = map_output(&response);

    let (status, incomplete_details) = map_status(response.stop_reason);
    let usage = map_usage(&response);

    Response {
        id: response.id,
        object: ResponseObjectType::Response,
        created_at: 0,
        status: Some(status),
        completed_at: None,
        error: None,
        incomplete_details,
        instructions: None,
        model: map_model(&response.model),
        output,
        output_text,
        usage: Some(usage),
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

fn map_output(response: &BetaMessage) -> (Vec<OutputItem>, Option<String>) {
    let mut output = Vec::new();
    let mut texts = Vec::new();

    for block in &response.content {
        match block {
            BetaContentBlock::Text(text) => texts.push(text.text.clone()),
            BetaContentBlock::Thinking(thinking) => texts.push(thinking.thinking.clone()),
            BetaContentBlock::RedactedThinking(thinking) => texts.push(thinking.data.clone()),
            BetaContentBlock::ToolUse(tool) => {
                output.push(OutputItem::Function(map_tool_use(tool)))
            }
            BetaContentBlock::ServerToolUse(tool) => {
                output.push(OutputItem::Function(map_server_tool_use(tool)))
            }
            BetaContentBlock::McpToolUse(tool) => {
                output.push(OutputItem::MCPCall(map_mcp_tool_use(tool)))
            }
            _ => {}
        }
    }

    let content = map_message_content(&texts, response.stop_reason);
    if let Some(message) = content {
        output.insert(0, OutputItem::Message(message));
    }

    let output_text = if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    };

    (output, output_text)
}

fn map_message_content(
    texts: &[String],
    stop_reason: Option<BetaStopReason>,
) -> Option<OutputMessage> {
    if texts.is_empty() && !matches!(stop_reason, Some(BetaStopReason::Refusal)) {
        return None;
    }

    let mut content = Vec::new();
    if matches!(stop_reason, Some(BetaStopReason::Refusal)) {
        let refusal = texts.join("\n");
        content.push(OutputMessageContent::Refusal(RefusalContent { refusal }));
    } else {
        let text = texts.join("\n");
        if !text.is_empty() {
            content.push(OutputMessageContent::OutputText(OutputTextContent {
                text,
                annotations: Vec::new(),
                logprobs: None,
            }));
        }
    }

    if content.is_empty() {
        None
    } else {
        Some(OutputMessage {
            id: "message".to_string(),
            r#type: OutputMessageType::Message,
            role: OutputMessageRole::Assistant,
            content,
            status: MessageStatus::Completed,
        })
    }
}

fn map_tool_use(tool: &BetaToolUseBlock) -> FunctionToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    FunctionToolCall {
        r#type: FunctionToolCallType::FunctionCall,
        id: Some(tool.id.clone()),
        call_id: tool.id.clone(),
        name: tool.name.clone(),
        arguments,
        status: Some(FunctionCallItemStatus::Completed),
    }
}

fn map_server_tool_use(tool: &BetaServerToolUseBlock) -> FunctionToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    FunctionToolCall {
        r#type: FunctionToolCallType::FunctionCall,
        id: Some(tool.id.clone()),
        call_id: tool.id.clone(),
        name: format!("{:?}", tool.name),
        arguments,
        status: Some(FunctionCallItemStatus::Completed),
    }
}

fn map_mcp_tool_use(tool: &BetaMcpToolUseBlock) -> MCPToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    MCPToolCall {
        r#type: MCPToolCallType::MCPCall,
        id: tool.id.clone(),
        server_label: tool.server_name.clone(),
        name: tool.name.clone(),
        arguments,
        output: None,
        error: None,
        status: MCPToolCallStatus::Completed,
        approval_request_id: None,
    }
}

fn map_status(
    stop_reason: Option<BetaStopReason>,
) -> (ResponseStatus, Option<ResponseIncompleteDetails>) {
    match stop_reason {
        Some(BetaStopReason::MaxTokens) | Some(BetaStopReason::ModelContextWindowExceeded) => (
            ResponseStatus::Incomplete,
            Some(ResponseIncompleteDetails {
                reason: ResponseIncompleteReason::MaxOutputTokens,
            }),
        ),
        _ => (ResponseStatus::Completed, None),
    }
}

fn map_usage(response: &BetaMessage) -> ResponseUsage {
    let input_tokens = response.usage.input_tokens as i64;
    let output_tokens = response.usage.output_tokens as i64;
    ResponseUsage {
        input_tokens,
        input_tokens_details: ResponseUsageInputTokensDetails { cached_tokens: 0 },
        output_tokens,
        output_tokens_details: ResponseUsageOutputTokensDetails {
            reasoning_tokens: 0,
        },
        total_tokens: input_tokens + output_tokens,
    }
}

fn map_model(model: &gproxy_protocol::claude::count_tokens::types::Model) -> String {
    match model {
        gproxy_protocol::claude::count_tokens::types::Model::Custom(value) => value.clone(),
        gproxy_protocol::claude::count_tokens::types::Model::Known(known) => {
            match serde_json::to_value(known) {
                Ok(JsonValue::String(value)) => value,
                _ => "unknown".to_string(),
            }
        }
    }
}
