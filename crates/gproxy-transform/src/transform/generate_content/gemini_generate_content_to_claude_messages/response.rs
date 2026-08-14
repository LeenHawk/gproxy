use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::gemini_content_to_claude_response_blocks;

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let mut candidates = input.candidates.into_iter();
    let first = candidates.next();
    let (content, stop_reason, stop_sequence) = if let Some(candidate) = first {
        let content = candidate
            .content
            .map(gemini_content_to_claude_response_blocks)
            .filter(|blocks| !blocks.is_empty())
            .unwrap_or_else(empty_text_response);
        let has_tool_use = content
            .iter()
            .any(|block| matches!(block, claude::ContentBlock::ToolUse(_)));
        (
            content,
            candidate
                .finish_reason
                .map(|reason| gemini_finish_reason_to_claude(reason, has_tool_use))
                .unwrap_or_else(|| claude::StopReason::Known(claude::StopReasonKnown::EndTurn)),
            candidate.finish_message,
        )
    } else {
        (
            empty_text_response(),
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
            None,
        )
    };

    Ok(crate::protocol::wire!(claude::CreateMessageResponseBody {
        id: input.response_id.unwrap_or_default(),
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content,
        model: input.model_version.unwrap_or_default().into(),
        stop_reason,
        stop_sequence,
        usage: input
            .usage_metadata
            .map(gemini_usage_to_claude)
            .unwrap_or_else(empty_usage),
        container: None,
        context_management: None,
        diagnostics: None,
        stop_details: None,
        extra: Default::default(),
    }))
}

fn gemini_finish_reason_to_claude(
    reason: gemini::FinishReason,
    has_tool_use: bool,
) -> claude::StopReason {
    let stop_reason = match reason {
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Safety
            | gemini::FinishReasonKnown::Recitation
            | gemini::FinishReasonKnown::Blocklist
            | gemini::FinishReasonKnown::ProhibitedContent
            | gemini::FinishReasonKnown::Spii
            | gemini::FinishReasonKnown::ImageSafety
            | gemini::FinishReasonKnown::ImageProhibitedContent,
        ) => claude::StopReason::Known(claude::StopReasonKnown::Refusal),
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::UnexpectedToolCall
            | gemini::FinishReasonKnown::TooManyToolCalls
            | gemini::FinishReasonKnown::MalformedFunctionCall,
        ) => claude::StopReason::Known(claude::StopReasonKnown::ToolUse),
        gemini::FinishReason::Known(_) | gemini::FinishReason::Unknown(_) => {
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    if has_tool_use
        && matches!(
            stop_reason,
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
        )
    {
        claude::StopReason::Known(claude::StopReasonKnown::ToolUse)
    } else {
        stop_reason
    }
}

pub(super) fn gemini_usage_to_claude(usage: gemini::UsageMetadata) -> claude::Usage {
    let speed = common::gemini_service_tier_to_claude_speed(usage.service_tier.clone());
    let service_tier = common::gemini_usage_service_tier_to_claude(usage.service_tier.clone());
    let cached = usage.cached_content_token_count.map(i32_to_u64);
    let thoughts = usage.thoughts_token_count.map(i32_to_u64);

    crate::protocol::wire!(claude::Usage {
        input_tokens: usage
            .prompt_token_count
            .map(i32_to_u64)
            .map(|tokens| tokens.saturating_sub(cached.unwrap_or_default())),
        output_tokens: usage
            .candidates_token_count
            .map(i32_to_u64)
            .map(|tokens| tokens.saturating_add(thoughts.unwrap_or_default())),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: cached,
        cache_creation: None,
        output_tokens_details: thoughts.map(|thinking_tokens| crate::protocol::wire!(
            claude::OutputTokensDetails {
                thinking_tokens,
                extra: Default::default(),
            }
        )),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier,
        speed,
        extra: Default::default(),
    })
}

fn empty_usage() -> claude::Usage {
    crate::protocol::wire!(claude::Usage {
        input_tokens: Some(0),
        output_tokens: Some(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    })
}

fn empty_text_response() -> Vec<claude::ContentBlock> {
    vec![claude::ContentBlock::Text(crate::protocol::wire!(
        claude::ResponseTextBlock {
            citations: None,
            text: String::new(),
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        }
    ))]
}

fn i32_to_u64(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn usage_separates_cached_input_and_includes_thinking_in_output() {
        let usage = gemini_usage_to_claude(crate::protocol::wire!(gemini::UsageMetadata {
            prompt_token_count: Some(100),
            cached_content_token_count: Some(60),
            candidates_token_count: Some(20),
            thoughts_token_count: Some(5),
            ..Default::default()
        }));
        assert_eq!(usage.input_tokens, Some(40));
        assert_eq!(usage.cache_read_input_tokens, Some(60));
        assert_eq!(usage.output_tokens, Some(25));
        assert_eq!(
            usage
                .output_tokens_details
                .map(|details| details.thinking_tokens),
            Some(5)
        );
    }

    #[test]
    fn function_call_with_stop_finishes_as_tool_use() {
        let input = serde_json::from_value(json!({
            "responseId": "r1",
            "candidates": [{
                "finishReason": "STOP",
                "content": {"role": "model", "parts": [{
                    "functionCall": {"id": "c1", "name": "echo", "args": {"x": 1}},
                    "thoughtSignature": "ciphertext"
                }]}
            }]
        }))
        .unwrap();
        let ctx = TransformContext::new(
            crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::GeminiGenerateContent,
            ),
            crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::ClaudeMessages,
            ),
        );
        let output = response(input, &ctx).unwrap();
        assert_eq!(
            output.stop_reason,
            claude::StopReason::Known(claude::StopReasonKnown::ToolUse)
        );
        let output = serde_json::to_value(output).unwrap();
        assert_eq!(
            output["content"][0]["caller"]["thought_signature"],
            "ciphertext"
        );
    }
}
