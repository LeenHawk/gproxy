use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<openai::CompactedResponseObject, TransformError> {
    let mut output = Vec::new();
    for (index, candidate) in input.candidates.into_iter().enumerate() {
        if let Some(content) = candidate.content {
            output.extend(gemini_content_to_compact_items(index, content));
        }
    }
    Ok(crate::protocol::wire!(openai::CompactedResponseObject {
        id: input.response_id.unwrap_or_default(),
        created_at: 0,
        object: openai::ResponseCompactionObjectType::ResponseCompaction,
        output,
        usage: input
            .usage_metadata
            .map(gemini_usage_to_response)
            .unwrap_or_else(default_usage),
        extra: Default::default(),
    }))
}

fn gemini_content_to_compact_items(
    index: usize,
    content: gemini::Content,
) -> Vec<openai::CompactResponseItem> {
    let mut items = Vec::new();
    let mut text_parts = Vec::new();
    for part in content.parts {
        let signature = part.thought_signature;
        match part.data {
            Some(gemini::PartData::Text { text })
                if part.thought == Some(true) || signature.is_some() =>
            {
                items.push(reasoning_item(
                    index,
                    (!text.is_empty()).then_some(text),
                    signature,
                ));
            }
            None if signature.is_some() => items.push(reasoning_item(index, None, signature)),
            Some(gemini::PartData::Text { text }) => {
                text_parts.push(openai::CompactMessageContentPart::Output(
                    openai::ResponseOutputContentPart::OutputText {
                        annotations: Vec::new(),
                        logprobs: None,
                        text,
                        extra: Default::default(),
                    },
                ))
            }
            Some(gemini::PartData::FunctionCall { function_call }) => {
                if signature.is_some() {
                    items.push(reasoning_item(index, None, signature));
                }
                let call_id = function_call
                    .id
                    .unwrap_or_else(|| format!("call_{}", function_call.name));
                items.push(openai::CompactResponseItem::Typed(
                    openai::TypedResponseItem::FunctionCall {
                        id: Some(call_id.clone()),
                        call_id,
                        name: function_call.name,
                        arguments: serde_json::to_string(&function_call.args.unwrap_or_default())
                            .unwrap_or_else(|_| "{}".to_owned()),
                        caller: None,
                        namespace: None,
                        status: Some(openai::ResponseItemLifecycleStatus::Completed),
                        extra: Default::default(),
                    },
                ));
            }
            _ => {}
        }
    }
    if !text_parts.is_empty() {
        items.push(openai::CompactResponseItem::Message(
            crate::protocol::wire!(openai::CompactMessageItem {
                id: format!("message_{index}"),
                type_: openai::ResponseMessageItemType::Message,
                content: text_parts,
                role: openai::CompactMessageRole::Assistant,
                status: openai::ResponseItemLifecycleStatus::Completed,
                phase: None,
                extra: Default::default(),
            }),
        ));
    }
    items
}

fn reasoning_item(
    index: usize,
    text: Option<String>,
    encrypted_content: Option<String>,
) -> openai::CompactResponseItem {
    openai::CompactResponseItem::Typed(openai::TypedResponseItem::Reasoning {
        id: Some(format!("rs_{index}")),
        summary: Vec::new(),
        content: text.map(|text| {
            vec![crate::protocol::wire!(openai::ResponseReasoningTextPart {
                text,
                type_: openai::ResponseReasoningTextType::ReasoningText,
                extra: Default::default(),
            })]
        }),
        encrypted_content,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        extra: Default::default(),
    })
}

fn gemini_usage_to_response(usage: gemini::UsageMetadata) -> openai::ResponseUsage {
    let input_tokens = usage.prompt_token_count.map(i32_to_u32).unwrap_or_default();
    let cached_tokens = usage
        .cached_content_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let reasoning_tokens = usage
        .thoughts_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let output_tokens = usage
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_default()
        .saturating_add(reasoning_tokens);
    let total_tokens = usage
        .total_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| input_tokens.saturating_add(output_tokens));
    crate::protocol::wire!(openai::ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        input_tokens_details: (cached_tokens > 0).then(|| crate::protocol::wire!(
            openai::ResponseInputTokensDetails {
                cache_write_tokens: 0,
                cached_tokens,
                extra: Default::default(),
            }
        )),
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

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_call_preserves_thought_signature() {
        let content: gemini::Content = serde_json::from_value(serde_json::json!({
            "role": "model",
            "parts": [{
                "functionCall": {
                    "id": "call_weather",
                    "name": "weather",
                    "args": {"city": "Shanghai"}
                },
                "thoughtSignature": "encrypted-reasoning"
            }]
        }))
        .expect("valid Gemini content");

        let items = gemini_content_to_compact_items(0, content);
        assert!(matches!(
            &items[0],
            openai::CompactResponseItem::Typed(openai::TypedResponseItem::Reasoning {
                encrypted_content: Some(signature),
                ..
            }) if signature == "encrypted-reasoning"
        ));
        assert!(matches!(
            &items[1],
            openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
                call_id,
                ..
            }) if call_id == "call_weather"
        ));
    }
}
