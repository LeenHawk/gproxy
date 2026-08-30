use crate::protocol::{gemini, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::gemini_content_to_chat_message;

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<openai::ChatCompletionResponse, TransformError> {
    let usage_metadata = input.usage_metadata;
    let service_tier = usage_metadata
        .as_ref()
        .and_then(|usage| common::gemini_service_tier_to_openai(usage.service_tier.clone()));

    Ok(crate::protocol::wire!(openai::ChatCompletionResponse {
        id: input.response_id.unwrap_or_default(),
        choices: input
            .candidates
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let message = candidate
                    .content
                    .map(gemini_content_to_chat_message)
                    .unwrap_or_else(empty_assistant_message);
                let has_tool_calls = message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| !calls.is_empty());
                crate::protocol::wire!(openai::ChatCompletionChoice {
                    finish_reason: candidate
                        .finish_reason
                        .map(|reason| gemini_finish_reason_to_chat(reason, has_tool_calls))
                        .unwrap_or(openai::ChatFinishReason::Stop),
                    index: candidate
                        .index
                        .map(i32_to_u32)
                        .unwrap_or_else(|| usize_to_u32(index)),
                    logprobs: None,
                    message,
                    extra: Default::default(),
                })
            })
            .collect(),
        created: 0,
        model: input
            .model_version
            .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned())
            .into(),
        object: openai::ChatCompletionObjectType::ChatCompletion,
        moderation: None,
        service_tier,
        system_fingerprint: None,
        usage: usage_metadata.map(gemini_usage_to_completion),
        extra: Default::default(),
    }))
}

fn empty_assistant_message() -> openai::ChatMessage {
    crate::protocol::wire!(openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: Some(String::new()),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: None,
        tool_calls: None,
        extra: Default::default(),
    })
}

fn gemini_finish_reason_to_chat(
    reason: gemini::FinishReason,
    has_tool_calls: bool,
) -> openai::ChatFinishReason {
    let finish_reason = match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
            openai::ChatFinishReason::Length
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => openai::ChatFinishReason::ContentFilter,
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::UnexpectedToolCall
            | gemini::FinishReasonKnown::TooManyToolCalls
            | gemini::FinishReasonKnown::MalformedFunctionCall,
        ) => openai::ChatFinishReason::ToolCalls,
        gemini::FinishReason::Known(_) | gemini::FinishReason::Unknown(_) => {
            openai::ChatFinishReason::Stop
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    if has_tool_calls && finish_reason == openai::ChatFinishReason::Stop {
        openai::ChatFinishReason::ToolCalls
    } else {
        finish_reason
    }
}

fn gemini_usage_to_completion(usage: gemini::UsageMetadata) -> openai::CompletionUsage {
    let prompt_tokens = usage.prompt_token_count.map(i32_to_u32).unwrap_or_default();
    let completion_tokens = usage
        .candidates_token_count
        .map(i32_to_u32)
        .unwrap_or_default();
    let total_tokens = usage
        .total_token_count
        .map(i32_to_u32)
        .unwrap_or_else(|| prompt_tokens.saturating_add(completion_tokens));

    crate::protocol::wire!(openai::CompletionUsage {
        completion_tokens,
        prompt_tokens,
        total_tokens,
        completion_tokens_details: usage.thoughts_token_count.map(|tokens| {
            crate::protocol::wire!(openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(i32_to_u32(tokens)),
                rejected_prediction_tokens: None,
                extra: Default::default(),
            })
        }),
        prompt_tokens_details: usage.cached_content_token_count.map(|tokens| {
            crate::protocol::wire!(openai::PromptTokensDetails {
                audio_tokens: None,
                cache_write_tokens: None,
                cached_tokens: Some(i32_to_u32(tokens)),
                extra: Default::default(),
            })
        }),
        extra: Default::default(),
    })
}

fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

fn usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

    fn ctx() -> TransformContext {
        TransformContext::new(
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChatCompletions,
            ),
        )
    }

    #[test]
    fn function_call_with_stop_finishes_as_tool_calls_and_preserves_signature() {
        let input = serde_json::from_value(json!({
            "responseId": "r1",
            "modelVersion": "gemini-test",
            "candidates": [{
                "index": 0,
                "finishReason": "STOP",
                "content": {"role": "model", "parts": [{
                    "functionCall": {"id": "call_1", "name": "weather", "args": {"city": "北京"}},
                    "thoughtSignature": "ciphertext"
                }]}
            }]
        }))
        .unwrap();

        let output = serde_json::to_value(response(input, &ctx()).unwrap()).unwrap();
        assert_eq!(output["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(
            output["choices"][0]["message"]["tool_calls"][0]["thought_signature"],
            "ciphertext"
        );
    }

    #[test]
    fn truncated_function_call_keeps_length_finish_reason() {
        let input = serde_json::from_value(json!({
            "candidates": [{
                "finishReason": "MAX_TOKENS",
                "content": {"role": "model", "parts": [{
                    "functionCall": {"id": "call_1", "name": "weather", "args": {}}
                }]}
            }]
        }))
        .unwrap();

        let output = response(input, &ctx()).unwrap();
        assert_eq!(
            output.choices[0].finish_reason,
            openai::ChatFinishReason::Length
        );
    }
}
