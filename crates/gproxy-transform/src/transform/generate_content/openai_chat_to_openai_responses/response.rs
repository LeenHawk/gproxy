use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

use super::content::chat_message_to_response_output_items;
use super::usage::chat_usage_to_response;

pub fn response(
    input: openai::ChatCompletionResponse,
    _: &TransformContext,
) -> Result<openai::ResponseObject, TransformError> {
    let mut output = Vec::new();
    let mut status = openai::ResponseStatus::Completed;
    let mut incomplete_details = None;
    let mut output_text = None;

    if let Some(choice) = input.choices.into_iter().next() {
        if matches!(
            choice.finish_reason,
            openai::ChatFinishReason::Length | openai::ChatFinishReason::ContentFilter
        ) {
            status = openai::ResponseStatus::Incomplete;
            incomplete_details = Some(openai::IncompleteDetails {
                reason: Some(match choice.finish_reason {
                    openai::ChatFinishReason::Length => openai::IncompleteReason::MaxOutputTokens,
                    openai::ChatFinishReason::ContentFilter => {
                        openai::IncompleteReason::ContentFilter
                    }
                    _ => openai::IncompleteReason::MaxOutputTokens,
                }),
                extra: Default::default(),
            });
        }
        output_text = choice
            .message
            .content
            .clone()
            .filter(|value| !value.is_empty());
        output.extend(chat_message_to_response_output_items(
            choice.index,
            choice.message,
        ));
    }

    Ok(openai::ResponseObject {
        id: input.id,
        created_at: input.created,
        background: None,
        completed_at: Some(input.created),
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: Some(input.model),
        moderation: None,
        object: openai::ResponseObjectType::Response,
        output,
        output_text,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: input.service_tier,
        status: Some(status),
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: chat_usage_to_response(input.usage),
        user: None,
        extra: Default::default(),
    })
}
