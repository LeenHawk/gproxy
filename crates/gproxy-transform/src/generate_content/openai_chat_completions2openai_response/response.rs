use gproxy_protocol::openai::create_chat_completions::response::{
    ChatCompletionChoice, ChatCompletionObjectType, CreateChatCompletionResponse,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionFinishReason, ChatCompletionMessageToolCall,
    ChatCompletionMessageToolCallFunction, ChatCompletionResponseMessage,
    ChatCompletionResponseRole, CompletionTokensDetails, CompletionUsage, PromptTokensDetails,
};
use gproxy_protocol::openai::create_response::response::Response;
use gproxy_protocol::openai::create_response::types::{
    CustomToolCall, FunctionToolCall, OutputItem, OutputMessageContent, ResponseIncompleteReason,
};

/// Convert an OpenAI responses response into an OpenAI chat-completions response.
pub fn transform_response(response: Response) -> CreateChatCompletionResponse {
    let (mut message_texts, refusal_texts, tool_calls) = extract_message_parts(&response.output);
    if message_texts.is_empty()
        && let Some(output_text) = &response.output_text
        && !output_text.is_empty()
    {
        message_texts.push(output_text.clone());
    }

    let content = if message_texts.is_empty() {
        None
    } else {
        Some(message_texts.join("\n"))
    };
    let refusal = if refusal_texts.is_empty() {
        None
    } else {
        Some(refusal_texts.join("\n"))
    };

    let message = ChatCompletionResponseMessage {
        role: ChatCompletionResponseRole::Assistant,
        content,
        refusal,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        annotations: None,
        function_call: None,
        audio: None,
    };

    let finish_reason = map_finish_reason(&response, &message);

    CreateChatCompletionResponse {
        id: response.id,
        object: ChatCompletionObjectType::ChatCompletion,
        created: response.created_at,
        model: response.model,
        choices: vec![ChatCompletionChoice {
            index: 0,
            message,
            finish_reason,
            logprobs: None,
        }],
        usage: response.usage.as_ref().map(map_usage),
        service_tier: response.service_tier,
        system_fingerprint: None,
    }
}

fn extract_message_parts(
    output: &[OutputItem],
) -> (Vec<String>, Vec<String>, Vec<ChatCompletionMessageToolCall>) {
    let mut texts = Vec::new();
    let mut refusals = Vec::new();
    let mut tool_calls = Vec::new();

    for item in output {
        match item {
            OutputItem::Message(message) => {
                for content in &message.content {
                    match content {
                        OutputMessageContent::OutputText(text) => {
                            if !text.text.is_empty() {
                                texts.push(text.text.clone());
                            }
                        }
                        OutputMessageContent::Refusal(refusal) => {
                            if !refusal.refusal.is_empty() {
                                refusals.push(refusal.refusal.clone());
                            }
                        }
                    }
                }
            }
            OutputItem::Function(function) => {
                if let Some(call) = map_function_call(function) {
                    tool_calls.push(call);
                }
            }
            OutputItem::CustomToolCall(custom) => {
                tool_calls.push(map_custom_call(custom));
            }
            _ => {}
        }
    }

    (texts, refusals, tool_calls)
}

fn map_function_call(call: &FunctionToolCall) -> Option<ChatCompletionMessageToolCall> {
    let id = call.id.clone().or_else(|| Some(call.call_id.clone()))?;
    Some(ChatCompletionMessageToolCall::Function {
        id,
        function: ChatCompletionMessageToolCallFunction {
            name: call.name.clone(),
            arguments: call.arguments.clone(),
        },
    })
}

fn map_custom_call(call: &CustomToolCall) -> ChatCompletionMessageToolCall {
    let id = call.id.clone().unwrap_or_else(|| call.call_id.clone());
    ChatCompletionMessageToolCall::Custom {
        id,
        custom: gproxy_protocol::openai::create_chat_completions::types::ChatCompletionMessageCustomToolCall {
            name: call.name.clone(),
            input: call.input.clone(),
        },
    }
}

fn map_finish_reason(
    response: &Response,
    message: &ChatCompletionResponseMessage,
) -> ChatCompletionFinishReason {
    if let Some(tool_calls) = &message.tool_calls
        && !tool_calls.is_empty()
    {
        return ChatCompletionFinishReason::ToolCalls;
    }

    if let Some(details) = &response.incomplete_details {
        return match details.reason {
            ResponseIncompleteReason::MaxOutputTokens => ChatCompletionFinishReason::Length,
            ResponseIncompleteReason::ContentFilter => ChatCompletionFinishReason::ContentFilter,
        };
    }

    ChatCompletionFinishReason::Stop
}

fn map_usage(
    usage: &gproxy_protocol::openai::create_response::types::ResponseUsage,
) -> CompletionUsage {
    CompletionUsage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        completion_tokens_details: Some(CompletionTokensDetails {
            accepted_prediction_tokens: None,
            audio_tokens: None,
            reasoning_tokens: Some(usage.output_tokens_details.reasoning_tokens),
            rejected_prediction_tokens: None,
        }),
        prompt_tokens_details: Some(PromptTokensDetails {
            audio_tokens: None,
            cached_tokens: Some(usage.input_tokens_details.cached_tokens),
        }),
    }
}
