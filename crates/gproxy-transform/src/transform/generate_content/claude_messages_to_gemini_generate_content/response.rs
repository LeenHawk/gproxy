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
    let cache_creation = usage.cache_creation_total();
    let prompt = (usage.input_tokens.is_some()
        || usage.cache_read_input_tokens.is_some()
        || cache_creation.is_some())
    .then(|| {
        u64_to_i32(
            usage
                .input_tokens
                .unwrap_or_default()
                .saturating_add(usage.cache_read_input_tokens.unwrap_or_default())
                .saturating_add(cache_creation.unwrap_or_default()),
        )
    });
    let cached = usage.cache_read_input_tokens.map(u64_to_i32);
    let output = usage.output_tokens.map(u64_to_i32);
    let thoughts = usage
        .output_tokens_details
        .map(|details| u64_to_i32(details.thinking_tokens));
    let candidates = output.map(|tokens| tokens.saturating_sub(thoughts.unwrap_or_default()));
    let total = prompt
        .unwrap_or_default()
        .saturating_add(output.unwrap_or_default());

    gemini::UsageMetadata {
        prompt_token_count: prompt,
        cached_content_token_count: cached,
        candidates_token_count: candidates,
        tool_use_prompt_token_count: None,
        thoughts_token_count: thoughts,
        total_token_count: Some(total),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        extra: Default::default(),
    }
}

fn u64_to_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_does_not_duplicate_thinking_in_candidates() {
        let usage = claude_usage_to_gemini(claude::Usage {
            input_tokens: Some(40),
            output_tokens: Some(25),
            cache_creation_input_tokens: Some(10),
            cache_read_input_tokens: Some(60),
            cache_creation: None,
            output_tokens_details: Some(claude::OutputTokensDetails {
                thinking_tokens: 5,
                extra: Default::default(),
            }),
            server_tool_use: None,
            iterations: None,
            inference_geo: None,
            service_tier: None,
            speed: None,
            extra: Default::default(),
        });
        assert_eq!(usage.prompt_token_count, Some(110));
        assert_eq!(usage.cached_content_token_count, Some(60));
        assert_eq!(usage.candidates_token_count, Some(20));
        assert_eq!(usage.thoughts_token_count, Some(5));
        assert_eq!(usage.total_token_count, Some(135));
    }
}
