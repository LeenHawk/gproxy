use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(in crate::generate_content) fn to_responses(
    usage: gemini::UsageMetadata,
) -> Result<openai::ResponseUsage, TransformError> {
    let input_tokens = required(usage.prompt_token_count, "promptTokenCount")?;
    let total = usage.total_token_count.map(nonnegative_value).transpose()?;
    let reasoning_tokens = usage
        .thoughts_token_count
        .map(nonnegative_value)
        .transpose()?;
    let candidates = usage
        .candidates_token_count
        .map(nonnegative_value)
        .transpose()?;
    let _tool_tokens = usage
        .tool_use_prompt_token_count
        .map(nonnegative_value)
        .transpose()?;
    validate_details(&usage.prompt_tokens_details)?;
    validate_details(&usage.cache_tokens_details)?;
    validate_details(&usage.candidates_tokens_details)?;
    validate_details(&usage.tool_use_prompt_tokens_details)?;
    let output_tokens = if let Some(total) = total {
        total.checked_sub(input_tokens).ok_or_else(|| {
            TransformError::shape("Gemini usageMetadata", "totalTokenCount is below input")
        })?
    } else if let Some(candidates) = candidates {
        candidates
            .checked_add(reasoning_tokens.unwrap_or(0))
            .ok_or_else(|| {
                TransformError::shape("Gemini usageMetadata", "output token count overflow")
            })?
    } else {
        return Err(TransformError::shape(
            "Gemini usageMetadata",
            "both totalTokenCount and candidatesTokenCount are missing",
        ));
    };
    let total_tokens = total.map(Ok).unwrap_or_else(|| {
        input_tokens.checked_add(output_tokens).ok_or_else(|| {
            TransformError::shape("Gemini usageMetadata", "total token count overflow")
        })
    })?;
    if let Some(candidates) = candidates {
        let declared_output = candidates
            .checked_add(reasoning_tokens.unwrap_or(0))
            .ok_or_else(|| {
                TransformError::shape("Gemini usageMetadata", "output token count overflow")
            })?;
        if declared_output != output_tokens {
            return Err(TransformError::shape(
                "Gemini usageMetadata",
                "totalTokenCount disagrees with candidate and thought counts",
            ));
        }
    }
    let cached_tokens = usage
        .cached_content_token_count
        .map(nonnegative_value)
        .transpose()?;
    Ok(crate::wire!(openai::ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens,
        input_tokens_details: cached_tokens.map(|cached_tokens| {
            openai::ResponseInputTokensDetails {
                cache_write_tokens: None,
                cached_tokens: Some(cached_tokens),
                rest: Default::default(),
            }
        }),
        output_tokens_details: reasoning_tokens.map(|reasoning_tokens| {
            openai::ResponseOutputTokensDetails {
                reasoning_tokens: Some(reasoning_tokens),
                rest: Default::default(),
            }
        }),
        rest: Default::default(),
    }))
}

fn validate_details(details: &[gemini::ModalityTokenCount]) -> Result<(), TransformError> {
    if details
        .iter()
        .filter_map(|detail| detail.token_count)
        .any(|count| count < 0)
    {
        Err(TransformError::shape(
            "Gemini usageMetadata",
            "modality token count must be nonnegative",
        ))
    } else {
        Ok(())
    }
}

fn required(value: Option<i32>, field: &str) -> Result<u32, TransformError> {
    value
        .ok_or_else(|| TransformError::shape("Gemini usageMetadata", format!("{field} is missing")))
        .and_then(nonnegative_value)
}

fn nonnegative_value(value: i32) -> Result<u32, TransformError> {
    u32::try_from(value).map_err(|_| {
        TransformError::shape("Gemini usageMetadata", "token count must be nonnegative")
    })
}
