use gproxy_protocol::openai::create_chat_completions::response::ChatCompletionChoice;
use gproxy_protocol::openai::create_chat_completions::response::CreateChatCompletionResponse;
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionMessageToolCall, ChatCompletionResponseMessage, CompletionUsage,
};
use gproxy_protocol::openai::create_response::response::{Response, ResponseObjectType};
use gproxy_protocol::openai::create_response::types::{
    CustomToolCall, CustomToolCallType, FunctionCallItemStatus, FunctionToolCall,
    FunctionToolCallType, MessageStatus, OutputItem, OutputMessage, OutputMessageContent,
    OutputMessageRole, OutputMessageType, ResponseStatus, ResponseUsage,
    ResponseUsageInputTokensDetails, ResponseUsageOutputTokensDetails,
};

/// Convert an OpenAI chat-completions response into an OpenAI responses response.
pub fn transform_response(response: CreateChatCompletionResponse) -> Response {
    let mut output = Vec::new();

    for choice in &response.choices {
        append_choice_output(choice, &mut output);
    }

    let usage = response.usage.as_ref().map(map_usage);
    let output_text = extract_output_text(&output);

    Response {
        id: response.id.clone(),
        object: ResponseObjectType::Response,
        created_at: response.created,
        status: Some(ResponseStatus::Completed),
        completed_at: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        model: response.model,
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
        service_tier: response.service_tier,
        prompt_cache_retention: None,
        store: None,
    }
}

fn append_choice_output(choice: &ChatCompletionChoice, output: &mut Vec<OutputItem>) {
    let message = &choice.message;

    if let Some(item) = map_message_to_output(message, choice.index) {
        output.push(OutputItem::Message(item));
    }

    if let Some(tool_calls) = &message.tool_calls {
        for call in tool_calls {
            if let Some(item) = map_tool_call_to_output(call) {
                output.push(item);
            }
        }
    }

    if let Some(function_call) = &message.function_call {
        let call_id = format!("function_call_{}", choice.index);
        output.push(OutputItem::Function(FunctionToolCall {
            r#type: FunctionToolCallType::FunctionCall,
            id: Some(call_id.clone()),
            call_id,
            name: function_call.name.clone(),
            arguments: function_call.arguments.clone(),
            status: Some(FunctionCallItemStatus::Completed),
        }));
    }
}

fn map_message_to_output(
    message: &ChatCompletionResponseMessage,
    index: i64,
) -> Option<OutputMessage> {
    let mut contents = Vec::new();

    if let Some(content) = &message.content
        && !content.is_empty()
    {
        contents.push(OutputMessageContent::OutputText(
            gproxy_protocol::openai::create_response::types::OutputTextContent {
                text: content.clone(),
                annotations: Vec::new(),
                logprobs: None,
            },
        ));
    }

    if let Some(refusal) = &message.refusal
        && !refusal.is_empty()
    {
        contents.push(OutputMessageContent::Refusal(
            gproxy_protocol::openai::create_response::types::RefusalContent {
                refusal: refusal.clone(),
            },
        ));
    }

    if contents.is_empty() {
        return None;
    }

    Some(OutputMessage {
        id: format!("message_{}", index),
        r#type: OutputMessageType::Message,
        role: OutputMessageRole::Assistant,
        content: contents,
        status: MessageStatus::Completed,
    })
}

fn map_tool_call_to_output(call: &ChatCompletionMessageToolCall) -> Option<OutputItem> {
    match call {
        ChatCompletionMessageToolCall::Function { id, function } => {
            Some(OutputItem::Function(FunctionToolCall {
                r#type: FunctionToolCallType::FunctionCall,
                id: Some(id.clone()),
                call_id: id.clone(),
                name: function.name.clone(),
                arguments: function.arguments.clone(),
                status: Some(FunctionCallItemStatus::Completed),
            }))
        }
        ChatCompletionMessageToolCall::Custom { id, custom } => {
            Some(OutputItem::CustomToolCall(CustomToolCall {
                r#type: CustomToolCallType::CustomToolCall,
                id: Some(id.clone()),
                call_id: id.clone(),
                name: custom.name.clone(),
                input: custom.input.clone(),
            }))
        }
    }
}

fn map_usage(usage: &CompletionUsage) -> ResponseUsage {
    let cached_tokens = usage
        .prompt_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens)
        .unwrap_or(0);
    let reasoning_tokens = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or(0);

    ResponseUsage {
        input_tokens: usage.prompt_tokens,
        input_tokens_details: ResponseUsageInputTokensDetails { cached_tokens },
        output_tokens: usage.completion_tokens,
        output_tokens_details: ResponseUsageOutputTokensDetails { reasoning_tokens },
        total_tokens: usage.total_tokens,
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
