use crate::protocol::{claude, gemini};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::content::claude_response_blocks_to_gemini_content;

pub fn response(
    input: claude::CreateMessageResponseBody,
    _: &TransformContext,
) -> Result<gemini::GenerateContentResponse, TransformError> {
    Ok(gemini::GenerateContentResponse {
        candidates: vec![gemini::Candidate {
            content: Some(claude_response_blocks_to_gemini_content(input.content)),
            finish_reason: Some(claude_stop_reason_to_gemini(input.stop_reason)),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: input.usage.output_tokens.map(u64_to_i32),
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: input.stop_sequence,
            extra: Default::default(),
        }],
        prompt_feedback: None,
        usage_metadata: Some(claude_usage_to_gemini(input.usage)),
        model_version: Some(common::claude_model_string(input.model)),
        response_id: Some(input.id),
        model_status: None,
        extra: Default::default(),
    })
}

fn claude_stop_reason_to_gemini(reason: claude::StopReason) -> gemini::FinishReason {
    let known = match reason {
        claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        | claude::StopReason::Known(claude::StopReasonKnown::ModelContextWindowExceeded) => {
            gemini::FinishReasonKnown::MaxTokens
        }
        claude::StopReason::Known(claude::StopReasonKnown::Refusal) => {
            gemini::FinishReasonKnown::Safety
        }
        claude::StopReason::Known(claude::StopReasonKnown::ToolUse) => {
            gemini::FinishReasonKnown::Stop
        }
        claude::StopReason::Known(_) | claude::StopReason::Unknown(_) => {
            gemini::FinishReasonKnown::Stop
        }
    };
    gemini::FinishReason::Known(known)
}

fn claude_usage_to_gemini(usage: claude::Usage) -> gemini::UsageMetadata {
    let prompt = usage.input_tokens.map(u64_to_i32);
    let cached = usage.cache_read_input_tokens.map(u64_to_i32);
    let output = usage.output_tokens.map(u64_to_i32);
    let thoughts = usage
        .output_tokens_details
        .map(|details| u64_to_i32(details.thinking_tokens));
    let total = prompt
        .unwrap_or_default()
        .saturating_add(output.unwrap_or_default());

    gemini::UsageMetadata {
        prompt_token_count: prompt,
        cached_content_token_count: cached,
        candidates_token_count: output,
        tool_use_prompt_token_count: None,
        thoughts_token_count: thoughts,
        total_token_count: Some(total),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        extra: Default::default(),
    }
}

fn u64_to_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}
