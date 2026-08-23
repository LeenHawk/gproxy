use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(super) fn convert(usage: claude::Usage) -> Result<gemini::UsageMetadata, TransformError> {
    let cache_creation = match usage.cache_creation_input_tokens {
        Some(tokens) => Some(tokens),
        None => usage
            .cache_creation
            .as_ref()
            .map(|cache| {
                checked_add(
                    cache.ephemeral_5m_input_tokens,
                    cache.ephemeral_1h_input_tokens,
                )
            })
            .transpose()?,
    };
    let prompt = usage
        .input_tokens
        .map(|tokens| {
            checked_add(tokens, usage.cache_read_input_tokens.unwrap_or(0))
                .and_then(|tokens| checked_add(tokens, cache_creation.unwrap_or(0)))
                .and_then(to_i32)
        })
        .transpose()?;
    let cached = usage.cache_read_input_tokens.map(to_i32).transpose()?;
    let output = usage.output_tokens.map(to_i32).transpose()?;
    let thoughts = usage
        .output_tokens_details
        .and_then(|details| details.thinking_tokens)
        .map(to_i32)
        .transpose()?;
    let candidates = output
        .map(|tokens| checked_sub(tokens, thoughts.unwrap_or(0)))
        .transpose()?;
    let total = prompt
        .zip(output)
        .map(|(prompt, output)| checked_add_i32(prompt, output))
        .transpose()?;
    Ok(gemini::UsageMetadata {
        prompt_token_count: prompt,
        cached_content_token_count: cached,
        candidates_token_count: candidates,
        tool_use_prompt_token_count: None,
        thoughts_token_count: thoughts,
        total_token_count: total,
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: service_tier(usage.service_tier, usage.speed),
        rest: usage.rest,
    })
}

fn service_tier(
    tier: Option<claude::UsageServiceTier>,
    speed: Option<claude::Speed>,
) -> Option<gemini::ServiceTier> {
    if matches!(speed, Some(claude::Speed::Known(claude::SpeedKnown::Fast))) {
        return Some(gemini::ServiceTier::Known(
            gemini::ServiceTierKnown::Priority,
        ));
    }
    tier.map(|tier| match tier {
        claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Priority) => {
            gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority)
        }
        claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Standard) => {
            gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard)
        }
        claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Batch) => {
            gemini::ServiceTier::Unknown("batch".into())
        }
        claude::UsageServiceTier::Unknown(value) => gemini::ServiceTier::Unknown(value),
        _ => gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified),
    })
}

fn to_i32(value: u64) -> Result<i32, TransformError> {
    i32::try_from(value)
        .map_err(|_| TransformError::shape("Claude usage", "token count exceeds i32"))
}

fn checked_add(left: u64, right: u64) -> Result<u64, TransformError> {
    left.checked_add(right)
        .ok_or_else(|| TransformError::shape("Claude usage", "token count overflow"))
}

fn checked_sub(left: i32, right: i32) -> Result<i32, TransformError> {
    left.checked_sub(right)
        .ok_or_else(|| TransformError::shape("Claude usage", "thinking exceeds output tokens"))
}

fn checked_add_i32(left: i32, right: i32) -> Result<i32, TransformError> {
    left.checked_add(right)
        .ok_or_else(|| TransformError::shape("Claude usage", "total token count exceeds i32"))
}
