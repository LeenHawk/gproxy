use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn service_tier(
    tier: Option<gemini::ServiceTier>,
) -> Result<Option<openai::ServiceTier>, TransformError> {
    let Some(tier) = tier else {
        return Ok(None);
    };
    Ok(match tier {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Flex) => openai::ServiceTier::Flex,
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority) => {
            openai::ServiceTier::Priority
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard) => {
            openai::ServiceTier::Default
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified)
        | gemini::ServiceTier::Unknown(_) => return Ok(None),
        _ => return Ok(None),
    }
    .into())
}

pub(crate) fn finish_reason(
    reason: gemini::FinishReason,
) -> Result<openai::ChatFinishReason, TransformError> {
    Ok(match reason {
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
        gemini::FinishReason::Known(gemini::FinishReasonKnown::UnexpectedToolCall) => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "UNEXPECTED_TOOL_CALL",
            ));
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::TooManyToolCalls) => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "TOO_MANY_TOOL_CALLS",
            ));
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MalformedFunctionCall) => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "MALFORMED_FUNCTION_CALL",
            ));
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop) => {
            openai::ChatFinishReason::Stop
        }
        gemini::FinishReason::Known(
            gemini::FinishReasonKnown::Language
            | gemini::FinishReasonKnown::ImageOther
            | gemini::FinishReasonKnown::NoImage
            | gemini::FinishReasonKnown::ImageRecitation,
        ) => openai::ChatFinishReason::ContentFilter,
        gemini::FinishReason::Known(gemini::FinishReasonKnown::FinishReasonUnspecified) => {
            openai::ChatFinishReason::Stop
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::Other) => {
            return Err(TransformError::unsupported("Gemini finish reason", "OTHER"));
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MissingThoughtSignature) => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "MISSING_THOUGHT_SIGNATURE",
            ));
        }
        gemini::FinishReason::Known(gemini::FinishReasonKnown::MalformedResponse) => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "MALFORMED_RESPONSE",
            ));
        }
        gemini::FinishReason::Unknown(value) => {
            return Err(TransformError::unsupported("Gemini finish reason", value));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Gemini finish reason",
                "future finish reason",
            ));
        }
    })
}

pub(crate) fn usage(
    usage: gemini::UsageMetadata,
) -> Result<openai::CompletionUsage, TransformError> {
    let prompt = required_count(usage.prompt_token_count, "promptTokenCount")?;
    let candidates = required_count(usage.candidates_token_count, "candidatesTokenCount")?;
    let thoughts = usage
        .thoughts_token_count
        .map(|value| count(value, "thoughtsTokenCount"))
        .transpose()?;
    let completion = candidates
        .checked_add(thoughts.unwrap_or(0))
        .ok_or_else(|| TransformError::shape("Gemini usage", "completion token sum overflow"))?;
    let expected_total = prompt
        .checked_add(completion)
        .ok_or_else(|| TransformError::shape("Gemini usage", "total token sum overflow"))?;
    let total = match usage.total_token_count {
        Some(total) => {
            let total = count(total, "totalTokenCount")?;
            if total != expected_total {
                return Err(TransformError::shape(
                    "Gemini usage",
                    "totalTokenCount does not equal prompt + thoughts + candidates",
                ));
            }
            total
        }
        None => expected_total,
    };
    Ok(crate::wire!(openai::CompletionUsage {
        completion_tokens: completion,
        prompt_tokens: prompt,
        total_tokens: total,
        completion_tokens_details: thoughts.map(|reasoning_tokens| {
            openai::CompletionTokensDetails {
                accepted_prediction_tokens: None,
                audio_tokens: None,
                reasoning_tokens: Some(reasoning_tokens),
                rejected_prediction_tokens: None,
                rest: Default::default(),
            }
        }),
        prompt_tokens_details: usage
            .cached_content_token_count
            .map(|value| {
                count(value, "cachedContentTokenCount").map(|cached_tokens| {
                    openai::PromptTokensDetails {
                        audio_tokens: None,
                        cache_write_tokens: None,
                        cached_tokens: Some(cached_tokens),
                        rest: Default::default(),
                    }
                })
            })
            .transpose()?,
        rest: Default::default(),
    }))
}

pub(crate) fn count(value: i32, field: &'static str) -> Result<u32, TransformError> {
    u32::try_from(value)
        .map_err(|_| TransformError::shape("Gemini usage", format!("{field} is negative")))
}

fn required_count(value: Option<i32>, field: &'static str) -> Result<u32, TransformError> {
    count(
        value
            .ok_or_else(|| TransformError::shape("Gemini usage", format!("{field} is missing")))?,
        field,
    )
}
