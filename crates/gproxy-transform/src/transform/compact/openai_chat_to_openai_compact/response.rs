use crate::protocol::openai;
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: openai::ChatCompletionResponse,
    _: &TransformContext,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let mut output = Vec::new();
    if let Some(choice) = input.choices.into_iter().next() {
        let logprobs = choice.logprobs.map(|logprobs| logprobs.content);
        output.extend(chat_message_to_compact_items(
            choice.index,
            choice.message,
            logprobs,
        ));
    }
    Ok(crate::protocol::wire!(openai::CompactedResponseObject {
        id: input.id,
        created_at: input.created,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output,
        usage: chat_usage_to_response(input.usage),
        extra: Default::default(),
    }))
}

fn chat_message_to_compact_items(
    index: u32,
    message: openai::ChatMessage,
    logprobs: Option<Vec<openai::TokenLogprob>>,
) -> Vec<openai::CompactResponseItem> {
    let mut items = Vec::new();
    let mut parts = Vec::new();
    let annotations = message
        .annotations
        .unwrap_or_default()
        .into_iter()
        .map(|annotation| openai::ResponseAnnotation::UrlCitation {
            end_index: annotation.url_citation.end_index,
            start_index: annotation.url_citation.start_index,
            title: annotation.url_citation.title,
            url: annotation.url_citation.url,
            extra: Default::default(),
        })
        .collect();
    if let Some(content) = message.content.filter(|text| !text.is_empty()) {
        parts.push(openai::CompactMessageContentPart::Output(
            openai::ResponseOutputContentPart::OutputText {
                annotations,
                logprobs,
                text: content,
                extra: Default::default(),
            },
        ));
    }
    if let Some(refusal) = message.refusal.filter(|text| !text.is_empty()) {
        parts.push(openai::CompactMessageContentPart::Output(
            openai::ResponseOutputContentPart::Refusal {
                refusal,
                extra: Default::default(),
            },
        ));
    }
    if !parts.is_empty() {
        items.push(openai::CompactResponseItem::Message(
            crate::protocol::wire!(openai::CompactMessageItem {
                id: format!("msg_{index}"),
                type_: openai::ResponseMessageItemType::Message,
                content: parts,
                role: openai::CompactMessageRole::Assistant,
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                extra: Default::default(),
            }),
        ));
    }
    if let Some(reasoning) = message.reasoning_content.filter(|text| !text.is_empty()) {
        items.push(openai::CompactResponseItem::Typed(
            openai::TypedResponseItem::Reasoning {
                id: Some(format!("reasoning_{index}")),
                summary: Vec::new(),
                content: Some(vec![crate::protocol::wire!(
                    openai::ResponseReasoningTextPart {
                        text: reasoning,
                        type_: openai::ResponseReasoningTextType::ReasoningText,
                        extra: Default::default(),
                    }
                )]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                extra: Default::default(),
            },
        ));
    }
    if let Some(call) = message.function_call {
        let call_id = format!("call_{}", call.name);
        items.push(function_call_item(call_id, call.name, call.arguments));
    }
    for call in message.tool_calls.into_iter().flatten() {
        match call {
            openai::ChatToolCall::Function { id, function, .. } => {
                items.push(function_call_item(id, function.name, function.arguments));
            }
            openai::ChatToolCall::Custom { id, custom, .. } => {
                items.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::CustomToolCall {
                        call_id: id,
                        input: custom.input,
                        name: custom.name,
                        id: None,
                        caller: None,
                        namespace: None,
                        extra: Default::default(),
                    },
                ));
            }
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        }
    }
    items
}

fn function_call_item(
    call_id: String,
    name: String,
    arguments: String,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
        id: Some(call_id.clone()),
        call_id,
        name,
        arguments,
        caller: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}

fn chat_usage_to_response(usage: Option<openai::CompletionUsage>) -> openai::ResponseUsage {
    let Some(usage) = usage else {
        return default_usage();
    };
    let (cached_tokens, cache_write_tokens) = usage
        .prompt_tokens_details
        .map(|details| (details.cached_tokens, details.cache_write_tokens))
        .unwrap_or_default();
    let reasoning_tokens = usage
        .completion_tokens_details
        .and_then(|details| details.reasoning_tokens)
        .unwrap_or_default();
    crate::protocol::wire!(openai::ResponseUsage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        input_tokens_details: (cached_tokens.is_some() || cache_write_tokens.is_some()).then(
            || crate::protocol::wire!(openai::ResponseInputTokensDetails {
                cache_write_tokens: cache_write_tokens.unwrap_or_default(),
                cached_tokens: cached_tokens.unwrap_or_default(),
                extra: Default::default(),
            })
        ),
        output_tokens_details: crate::protocol::wire!(openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

fn default_usage() -> openai::ResponseUsage {
    crate::protocol::wire!(openai::ResponseUsage {
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        input_tokens_details: None,
        output_tokens_details: crate::protocol::wire!(openai::ResponseOutputTokensDetails {
            reasoning_tokens: 0,
            extra: Default::default(),
        }),
        extra: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_chat_content_logprobs() {
        let message: openai::ChatMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": "hello"
        }))
        .unwrap();
        let logprobs: Vec<openai::TokenLogprob> = serde_json::from_value(serde_json::json!([{
            "token": "hello",
            "bytes": [104, 101, 108, 108, 111],
            "logprob": -0.1,
            "top_logprobs": []
        }]))
        .unwrap();

        let items = chat_message_to_compact_items(0, message, Some(logprobs));
        let openai::CompactResponseItem::Message(message) = &items[0] else {
            panic!("expected compact message");
        };
        assert!(matches!(
            &message.content[0],
            openai::CompactMessageContentPart::Output(
                openai::ResponseOutputContentPart::OutputText {
                    logprobs: Some(logprobs),
                    ..
                }
            ) if logprobs.len() == 1 && logprobs[0].token == "hello"
        ));
    }
}
