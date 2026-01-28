use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaContentBlock, BetaMessage, BetaStopReason, BetaToolUseBlock,
};
use gproxy_protocol::openai::create_chat_completions::response::{
    ChatCompletionChoice, CreateChatCompletionResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCall,
    ChatCompletionMessageToolCallFunction, ChatCompletionResponseMessage,
    ChatCompletionResponseRole, CompletionUsage,
};
use serde_json::Value as JsonValue;

/// Convert a Claude message response into an OpenAI chat-completions response.
pub fn transform_response(response: ClaudeCreateMessageResponse) -> CreateChatCompletionResponse {
    let (content, tool_calls, refusal) = map_content(&response.content, response.stop_reason);

    let message = ChatCompletionResponseMessage {
        role: ChatCompletionResponseRole::Assistant,
        content,
        refusal,
        tool_calls,
        annotations: None,
        function_call: None,
        audio: None,
    };

    let choice = ChatCompletionChoice {
        index: 0,
        message,
        finish_reason: map_finish_reason(response.stop_reason),
        logprobs: None,
    };

    let usage = map_usage(&response);

    CreateChatCompletionResponse {
        id: response.id,
        object: gproxy_protocol::openai::create_chat_completions::response::ChatCompletionObjectType::ChatCompletion,
        created: 0,
        model: map_model(&response.model),
        choices: vec![choice],
        usage,
        service_tier: None,
        system_fingerprint: None,
    }
}

fn map_content(
    blocks: &[BetaContentBlock],
    stop_reason: Option<BetaStopReason>,
) -> (
    Option<String>,
    Option<Vec<ChatCompletionMessageToolCall>>,
    Option<String>,
) {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            BetaContentBlock::Text(text) => texts.push(text.text.clone()),
            BetaContentBlock::Thinking(thinking) => {
                texts.push(thinking.thinking.clone());
            }
            BetaContentBlock::RedactedThinking(thinking) => {
                texts.push(thinking.data.clone());
            }
            BetaContentBlock::ToolUse(tool) => {
                tool_calls.push(map_tool_use(tool));
            }
            BetaContentBlock::ServerToolUse(tool) => {
                tool_calls.push(map_server_tool_use(tool));
            }
            BetaContentBlock::McpToolUse(tool) => {
                tool_calls.push(map_mcp_tool_use(tool));
            }
            _ => {}
        }
    }

    let text = if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    };

    let tool_calls = if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    };

    let refusal = if matches!(stop_reason, Some(BetaStopReason::Refusal)) {
        text.clone()
    } else {
        None
    };

    let content = if refusal.is_some() { None } else { text };

    (content, tool_calls, refusal)
}

fn map_tool_use(tool: &BetaToolUseBlock) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: tool.name.clone(),
            arguments,
        },
    }
}

fn map_server_tool_use(
    tool: &gproxy_protocol::claude::create_message::types::BetaServerToolUseBlock,
) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: format!("{:?}", tool.name),
            arguments,
        },
    }
}

fn map_mcp_tool_use(
    tool: &gproxy_protocol::claude::create_message::types::BetaMcpToolUseBlock,
) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: tool.name.clone(),
            arguments,
        },
    }
}

fn map_finish_reason(reason: Option<BetaStopReason>) -> ChatCompletionFinishReason {
    match reason {
        Some(BetaStopReason::MaxTokens) | Some(BetaStopReason::ModelContextWindowExceeded) => {
            ChatCompletionFinishReason::Length
        }
        Some(BetaStopReason::ToolUse) => ChatCompletionFinishReason::ToolCalls,
        Some(BetaStopReason::Refusal) => ChatCompletionFinishReason::ContentFilter,
        Some(BetaStopReason::StopSequence) | Some(BetaStopReason::EndTurn) => {
            ChatCompletionFinishReason::Stop
        }
        Some(BetaStopReason::PauseTurn) | None => ChatCompletionFinishReason::Stop,
    }
}

fn map_usage(response: &BetaMessage) -> Option<CompletionUsage> {
    Some(CompletionUsage {
        prompt_tokens: response.usage.input_tokens as i64,
        completion_tokens: response.usage.output_tokens as i64,
        total_tokens: (response.usage.input_tokens + response.usage.output_tokens) as i64,
        completion_tokens_details: None,
        prompt_tokens_details: None,
    })
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
