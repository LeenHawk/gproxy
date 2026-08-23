use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(super) fn convert(usage: gemini::UsageMetadata) -> Result<claude::Usage, TransformError> {
    if usage.tool_use_prompt_token_count.is_some()
        || !usage.prompt_tokens_details.is_empty()
        || !usage.cache_tokens_details.is_empty()
        || !usage.candidates_tokens_details.is_empty()
        || !usage.tool_use_prompt_tokens_details.is_empty()
        || !usage.rest.is_empty()
    {
        return Err(TransformError::unsupported(
            "Gemini usage",
            "usage details without a Claude counterpart",
        ));
    }
    let cached = usage.cached_content_token_count.map(to_u64).transpose()?;
    let thoughts = usage.thoughts_token_count.map(to_u64).transpose()?;
    let input_tokens = usage
        .prompt_token_count
        .map(to_u64)
        .transpose()?
        .map(|tokens| checked_sub(tokens, cached.unwrap_or(0)))
        .transpose()?;
    let output_tokens = usage
        .candidates_token_count
        .map(to_u64)
        .transpose()?
        .map(|tokens| checked_add(tokens, thoughts.unwrap_or(0)))
        .transpose()?;
    if let Some(total) = usage.total_token_count.map(to_u64).transpose()? {
        let prompt = usage
            .prompt_token_count
            .map(to_u64)
            .transpose()?
            .unwrap_or(0);
        let candidates = usage
            .candidates_token_count
            .map(to_u64)
            .transpose()?
            .unwrap_or(0);
        let expected = checked_add(checked_add(prompt, candidates)?, thoughts.unwrap_or(0))?;
        if total != expected {
            return Err(TransformError::shape(
                "Gemini usage",
                "totalTokenCount is inconsistent",
            ));
        }
    }
    Ok(claude::Usage {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: cached,
        cache_creation: None,
        output_tokens_details: thoughts.map(|thinking_tokens| claude::OutputTokensDetails {
            thinking_tokens: Some(thinking_tokens),
            rest: Default::default(),
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: service_tier(usage.service_tier.clone()),
        speed: matches!(
            usage.service_tier,
            Some(gemini::ServiceTier::Known(
                gemini::ServiceTierKnown::Priority
            ))
        )
        .then_some(claude::Speed::Known(claude::SpeedKnown::Fast)),
        rest: usage.rest,
    })
}

fn service_tier(tier: Option<gemini::ServiceTier>) -> Option<claude::UsageServiceTier> {
    tier.map(|tier| match tier {
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Standard) => {
            claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Standard)
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Priority) => {
            claude::UsageServiceTier::Known(claude::UsageServiceTierKnown::Priority)
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Flex) => {
            claude::UsageServiceTier::Unknown("flex".into())
        }
        gemini::ServiceTier::Known(gemini::ServiceTierKnown::Unspecified) => {
            claude::UsageServiceTier::Unknown("unspecified".into())
        }
        gemini::ServiceTier::Unknown(value) => claude::UsageServiceTier::Unknown(value),
        _ => claude::UsageServiceTier::Unknown("unknown".into()),
    })
}

fn to_u64(value: i32) -> Result<u64, TransformError> {
    u64::try_from(value)
        .map_err(|_| TransformError::shape("Gemini usage", "token count is negative"))
}

fn checked_sub(left: u64, right: u64) -> Result<u64, TransformError> {
    left.checked_sub(right)
        .ok_or_else(|| TransformError::shape("Gemini usage", "cached exceeds prompt tokens"))
}

fn checked_add(left: u64, right: u64) -> Result<u64, TransformError> {
    left.checked_add(right)
        .ok_or_else(|| TransformError::shape("Gemini usage", "output token count overflow"))
}
