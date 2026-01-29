use std::collections::BTreeMap;

use gproxy_protocol::claude::count_tokens::types::Model as ClaudeModel;
use gproxy_protocol::claude::create_message::response::CreateMessageResponse as ClaudeCreateMessageResponse;
use gproxy_protocol::claude::create_message::types::{
    BetaCacheCreation, BetaContentBlock, BetaMessage, BetaMessageRole, BetaMessageType,
    BetaServerToolUsage, BetaServiceTierUsed, BetaStopReason, BetaTextBlock, BetaTextBlockType,
    BetaToolUseBlock, BetaToolUseBlockType, BetaUsage,
};
use gproxy_protocol::openai::create_chat_completions::response::CreateChatCompletionResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCall,
    ChatCompletionMessageToolCallFunction, ChatCompletionResponseMessage,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI chat-completions response into a Claude message response.
pub fn transform_response(response: CreateChatCompletionResponse) -> ClaudeCreateMessageResponse {
    let choice = response.choices.first();

    let (content, stop_reason) = match choice {
        Some(choice) => {
            let blocks = map_response_message(&choice.message);
            let stop_reason = map_finish_reason(choice.finish_reason);
            (blocks, stop_reason)
        }
        None => (Vec::new(), None),
    };

    let usage = map_usage(response.usage);

    BetaMessage {
        id: response.id,
        container: None,
        content,
        context_management: None,
        model: ClaudeModel::Custom(response.model),
        role: BetaMessageRole::Assistant,
        stop_reason,
        stop_sequence: None,
        r#type: BetaMessageType::Message,
        usage,
    }
}

fn map_response_message(message: &ChatCompletionResponseMessage) -> Vec<BetaContentBlock> {
    let mut blocks = Vec::new();

    if let Some(content) = &message.content
        && !content.is_empty()
    {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text: content.clone(),
            r#type: BetaTextBlockType::Text,
        }));
    }

    if let Some(refusal) = &message.refusal
        && !refusal.is_empty()
    {
        blocks.push(BetaContentBlock::Text(BetaTextBlock {
            citations: None,
            text: refusal.clone(),
            r#type: BetaTextBlockType::Text,
        }));
    }

    if let Some(tool_calls) = &message.tool_calls {
        for tool_call in tool_calls {
            blocks.push(BetaContentBlock::ToolUse(map_tool_call(tool_call)));
        }
    }

    if let Some(function_call) = &message.function_call {
        let tool_call = ChatCompletionMessageToolCall::Function {
            id: "function_call".to_string(),
            function: ChatCompletionMessageToolCallFunction {
                name: function_call.name.clone(),
                arguments: function_call.arguments.clone(),
            },
        };
        blocks.push(BetaContentBlock::ToolUse(map_tool_call(&tool_call)));
    }

    blocks
}

fn map_tool_call(tool_call: &ChatCompletionMessageToolCall) -> BetaToolUseBlock {
    let (id, name, input) = match tool_call {
        ChatCompletionMessageToolCall::Function { id, function } => {
            let input = parse_tool_arguments(&function.arguments);
            (id.clone(), function.name.clone(), input)
        }
        ChatCompletionMessageToolCall::Custom { id, custom } => {
            let mut input = BTreeMap::new();
            input.insert("input".to_string(), JsonValue::String(custom.input.clone()));
            (id.clone(), custom.name.clone(), input)
        }
    };

    BetaToolUseBlock {
        id,
        input,
        name,
        r#type: BetaToolUseBlockType::ToolUse,
        caller: None,
    }
}

fn parse_tool_arguments(arguments: &str) -> BTreeMap<String, JsonValue> {
    match serde_json::from_str::<JsonValue>(arguments) {
        Ok(JsonValue::Object(map)) => map.into_iter().collect(),
        Ok(other) => {
            let mut map = BTreeMap::new();
            map.insert("arguments".to_string(), other);
            map
        }
        Err(_) => {
            let mut map = BTreeMap::new();
            map.insert(
                "arguments".to_string(),
                JsonValue::String(arguments.to_string()),
            );
            map
        }
    }
}

fn map_finish_reason(reason: ChatCompletionFinishReason) -> Option<BetaStopReason> {
    Some(match reason {
        ChatCompletionFinishReason::Stop => BetaStopReason::EndTurn,
        ChatCompletionFinishReason::Length => BetaStopReason::MaxTokens,
        ChatCompletionFinishReason::ToolCalls | ChatCompletionFinishReason::FunctionCall => {
            BetaStopReason::ToolUse
        }
        ChatCompletionFinishReason::ContentFilter => BetaStopReason::Refusal,
    })
}

fn map_usage(
    usage: Option<gproxy_protocol::openai::create_chat_completions::types::CompletionUsage>,
) -> BetaUsage {
    let (input_tokens, output_tokens) = match usage {
        Some(usage) => (
            usage.prompt_tokens.max(0) as u32,
            usage.completion_tokens.max(0) as u32,
        ),
        None => (0, 0),
    };

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
