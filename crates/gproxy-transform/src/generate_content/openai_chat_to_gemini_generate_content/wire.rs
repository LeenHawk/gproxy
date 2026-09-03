use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn service_tier(tier: Option<openai::ServiceTier>) -> Option<gemini::ServiceTier> {
    Some(gemini::ServiceTier::Known(match tier? {
        openai::ServiceTier::Auto => gemini::ServiceTierKnown::Unspecified,
        openai::ServiceTier::Flex => gemini::ServiceTierKnown::Flex,
        openai::ServiceTier::Fast
        | openai::ServiceTier::Priority
        | openai::ServiceTier::Ultrafast => gemini::ServiceTierKnown::Priority,
        openai::ServiceTier::Default
        | openai::ServiceTier::Scale
        | openai::ServiceTier::OnDemand => gemini::ServiceTierKnown::Standard,
        openai::ServiceTier::Unknown(_) => return None,
    }))
}

pub(crate) fn finish_reason(
    reason: openai::ChatFinishReason,
) -> Result<gemini::FinishReason, TransformError> {
    let reason = match reason {
        openai::ChatFinishReason::Length => gemini::FinishReasonKnown::MaxTokens,
        openai::ChatFinishReason::ContentFilter => gemini::FinishReasonKnown::Safety,
        openai::ChatFinishReason::Stop
        | openai::ChatFinishReason::ToolCalls
        | openai::ChatFinishReason::FunctionCall => gemini::FinishReasonKnown::Stop,
        openai::ChatFinishReason::Unknown(value) => {
            return Err(TransformError::unsupported("Chat finish reason", value));
        }
    };
    Ok(gemini::FinishReason::Known(reason))
}

pub(crate) fn usage(
    usage: openai::CompletionUsage,
) -> Result<gemini::UsageMetadata, TransformError> {
    let reasoning = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens);
    let candidates = usage
        .completion_tokens
        .checked_sub(reasoning.unwrap_or(0))
        .ok_or_else(|| {
            TransformError::shape("Chat usage", "reasoning tokens exceed completion tokens")
        })?;
    let expected_total = usage
        .prompt_tokens
        .checked_add(usage.completion_tokens)
        .ok_or_else(|| TransformError::shape("Chat usage", "total token sum overflow"))?;
    if usage.total_tokens != expected_total {
        return Err(TransformError::shape(
            "Chat usage",
            "total_tokens does not equal prompt_tokens + completion_tokens",
        ));
    }
    Ok(crate::wire!(gemini::UsageMetadata {
        prompt_token_count: Some(signed(usage.prompt_tokens, "prompt_tokens")?),
        cached_content_token_count: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens)
            .map(|value| signed(value, "cached_tokens"))
            .transpose()?,
        candidates_token_count: Some(signed(candidates, "candidate tokens")?),
        tool_use_prompt_token_count: None,
        thoughts_token_count: reasoning
            .map(|value| signed(value, "reasoning_tokens"))
            .transpose()?,
        total_token_count: Some(signed(usage.total_tokens, "total_tokens")?),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        rest: Default::default(),
    }))
}

pub(crate) fn index(value: u32) -> Result<i32, TransformError> {
    signed(value, "choice index")
}

fn signed(value: u32, field: &'static str) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Chat wire", format!("{field} exceeds i32")))
}
