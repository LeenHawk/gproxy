use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::content::response_output_items_to_chat_message;
use super::usage::response_usage_to_chat;

pub fn response(
    input: openai::ResponseObject,
    _: &TransformContext,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    let finish_reason = finish_reason(&input);
    let message = response_output_items_to_chat_message(input.output, input.output_text);

    Ok(openai::ChatCompletionResponse {
        id: input.id,
        choices: vec![openai::ChatCompletionChoice {
            finish_reason,
            index: 0,
            logprobs: None,
            message,
            extra: Default::default(),
        }],
        created: input.created_at,
        model: input.model.unwrap_or_else(default_model),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier: input.service_tier,
        system_fingerprint: None,
        usage: response_usage_to_chat(input.usage),
        extra: Default::default(),
    })
}

fn finish_reason(response: &openai::ResponseObject) -> openai::ChatFinishReason {
    if response.output.iter().any(|item| {
        matches!(
            &item.0,
            openai::ResponseItem::Typed(
                openai::TypedResponseItem::FunctionCall { .. }
                    | openai::TypedResponseItem::CustomToolCall { .. }
            )
        )
    }) {
        return openai::ChatFinishReason::ToolCalls;
    }

    match response
        .incomplete_details
        .as_ref()
        .and_then(|details| details.reason.as_ref())
    {
        Some(openai::IncompleteReason::MaxOutputTokens) => openai::ChatFinishReason::Length,
        Some(openai::IncompleteReason::ContentFilter) => openai::ChatFinishReason::ContentFilter,
        None => openai::ChatFinishReason::Stop,
    }
}

fn default_model() -> openai::OpenAiModelId {
    openai::OpenAiModelId::Unknown("unknown".to_owned())
}
