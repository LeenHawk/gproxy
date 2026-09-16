use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::responses::response as r},
};

#[derive(Debug, Default, Clone, Copy)]
pub struct GeminiUsageFacts {
    pub cache_write_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
}

fn nonnegative(n: i64) -> Result<i64, TransformError> {
    Ok(n)
}

fn count(v: Option<i64>, field: &str) -> Result<i64, TransformError> {
    nonnegative(v.ok_or_else(|| TransformError::missing_metadata(field))?)
}

fn add(a: i64, b: i64) -> Result<i64, TransformError> {
    nonnegative(a)?
        .checked_add(nonnegative(b)?)
        .ok_or_else(|| TransformError::invalid_result("usage", "token overflow"))
}

pub(super) fn to_responses(
    input: g::UsageMetadata,
    facts: GeminiUsageFacts,
    report: &mut Report,
) -> Result<r::ResponseUsage, TransformError> {
    let prompt = add(
        count(input.prompt_token_count, "prompt_token_count")?,
        input.tool_use_prompt_token_count.unwrap_or(0),
    )?;
    let visible = count(input.candidates_token_count, "candidates_token_count")?;
    let total = count(input.total_token_count, "total_token_count")?;
    let output = total.checked_sub(prompt).ok_or_else(|| {
        TransformError::invalid_result("usage", "inconsistent input/output total")
    })?;
    let thinking = output - visible;

    let cached = count(
        input.cached_content_token_count.or(facts.cached_tokens),
        "cached_content_token_count actual fact",
    )?;
    let written = count(facts.cache_write_tokens, "cache_write_tokens actual fact")?;

    if input.prompt_tokens_details.is_some()
        || input.cache_tokens_details.is_some()
        || input.candidates_tokens_details.is_some()
        || input.tool_use_prompt_tokens_details.is_some()
    {
        report.omitted(
            "usage.modality_details",
            "Responses has no Gemini modality breakdown",
        );
    }
    Ok(r::ResponseUsage::builder(
        prompt,
        r::ResponseInputTokensDetails::builder(written, cached).build(),
        output,
        r::ResponseOutputTokensDetails::builder(thinking).build(),
        total,
    )
    .build())
}

pub(super) fn to_gemini(
    input: r::ResponseUsage,
    report: &mut Report,
) -> Result<g::UsageMetadata, TransformError> {
    let thinking = nonnegative(input.output_tokens_details.reasoning_tokens)?;
    let visible = input.output_tokens.checked_sub(thinking).ok_or_else(|| {
        TransformError::invalid_result("usage.reasoning", "reasoning exceeds output")
    })?;

    let mut out = g::UsageMetadata::builder().build();
    out.prompt_token_count = Some(input.input_tokens);
    out.candidates_token_count = Some(visible);
    out.thoughts_token_count = Some(thinking);
    out.total_token_count = Some(input.total_tokens);
    out.cached_content_token_count = Some(input.input_tokens_details.cached_tokens);
    report.omitted(
        "usage.cache_write_tokens",
        "Gemini has no distinct cache-write counter",
    );
    Ok(out)
}
