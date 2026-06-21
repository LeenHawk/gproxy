use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::content::gemini_content_to_claude_response_blocks;

pub fn response(
    input: gemini::GenerateContentResponse,
    _: &TransformContext,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let mut candidates = input.candidates.into_iter();
    let first = candidates.next();
    let (content, stop_reason, stop_sequence) = if let Some(candidate) = first {
        (
            candidate
                .content
                .map(gemini_content_to_claude_response_blocks)
                .filter(|blocks| !blocks.is_empty())
                .unwrap_or_else(empty_text_response),
            candidate
                .finish_reason
                .map(gemini_finish_reason_to_claude)
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

    Ok(claude::CreateMessageResponseBody {
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
    })
}

fn gemini_finish_reason_to_claude(reason: gemini::FinishReason) -> claude::StopReason {
    match reason {
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
    }
}

fn gemini_usage_to_claude(usage: gemini::UsageMetadata) -> claude::Usage {
    claude::Usage {
        input_tokens: usage.prompt_token_count.map(i32_to_u64),
        output_tokens: usage.candidates_token_count.map(i32_to_u64),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: usage.cached_content_token_count.map(i32_to_u64),
        cache_creation: None,
        output_tokens_details: usage.thoughts_token_count.map(|tokens| {
            claude::OutputTokensDetails {
                thinking_tokens: i32_to_u64(tokens),
                extra: Default::default(),
            }
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    }
}

fn empty_usage() -> claude::Usage {
    claude::Usage {
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
    }
}

fn empty_text_response() -> Vec<claude::ContentBlock> {
    vec![claude::ContentBlock::Text(claude::ResponseTextBlock {
        citations: None,
        text: String::new(),
        type_: claude::TextBlockType::Text,
        extra: Default::default(),
    })]
}

fn i32_to_u64(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}
