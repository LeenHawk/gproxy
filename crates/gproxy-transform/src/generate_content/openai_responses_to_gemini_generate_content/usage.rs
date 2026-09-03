use gproxy_protocol::{gemini, openai};

pub(in crate::generate_content) fn to_gemini(
    usage: Option<openai::ResponseUsage>,
) -> Result<Option<gemini::UsageMetadata>, crate::TransformError> {
    let Some(usage) = usage else {
        return Ok(None);
    };
    let cached_tokens = usage
        .input_tokens_details
        .as_ref()
        .and_then(|details| details.cached_tokens);
    let reasoning_tokens = usage
        .output_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens);
    let expected_total = usage
        .input_tokens
        .checked_add(usage.output_tokens)
        .ok_or_else(|| {
            crate::TransformError::shape("Responses usage", "total token count overflow")
        })?;
    if expected_total != usage.total_tokens {
        return Err(crate::TransformError::shape(
            "Responses usage",
            "total_tokens disagrees with input and output counts",
        ));
    }
    let candidate_tokens = usage
        .output_tokens
        .checked_sub(reasoning_tokens.unwrap_or(0))
        .ok_or_else(|| {
            crate::TransformError::shape(
                "Responses usage",
                "output_tokens is below reasoning_tokens",
            )
        })?;
    Ok(Some(gemini::UsageMetadata {
        prompt_token_count: Some(to_i32(usage.input_tokens)?),
        cached_content_token_count: cached_tokens.map(to_i32).transpose()?,
        candidates_token_count: Some(to_i32(candidate_tokens)?),
        tool_use_prompt_token_count: None,
        thoughts_token_count: reasoning_tokens.map(to_i32).transpose()?,
        total_token_count: Some(to_i32(usage.total_tokens)?),
        prompt_tokens_details: Vec::new(),
        cache_tokens_details: Vec::new(),
        candidates_tokens_details: Vec::new(),
        tool_use_prompt_tokens_details: Vec::new(),
        service_tier: None,
        rest: Default::default(),
    }))
}

fn to_i32(value: u32) -> Result<i32, crate::TransformError> {
    i32::try_from(value)
        .map_err(|_| crate::TransformError::shape("Responses usage", "token count exceeds i32"))
}
