use crate::{
    transform::{Report, TransformError},
    wire::{claude::generate_content as c, gemini as g},
};
/// Measured counters absent from the source response. Supplied counters must
/// agree with any corresponding upstream fields; omission is not zero.
#[derive(Debug, Default, Clone, Copy)]
pub struct ClaudeGeminiUsageFacts {
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub thinking_tokens: Option<i64>,
}
fn count(v: i64) -> Result<i64, TransformError> {
    Ok(v)
}
fn sum(a: i64, b: i64) -> Result<i64, TransformError> {
    count(a)?
        .checked_add(count(b)?)
        .ok_or_else(|| TransformError::invalid_result("usage", "count overflow"))
}
fn actual(a: Option<i64>, b: Option<i64>, field: &str) -> Result<i64, TransformError> {
    count(
        a.or(b)
            .ok_or_else(|| TransformError::missing_metadata(field))?,
    )
}
pub(super) fn to_gemini(
    input: c::Usage,
    facts: ClaudeGeminiUsageFacts,
    report: &mut Report,
) -> Result<g::UsageMetadata, TransformError> {
    let breakdown = input
        .cache_creation
        .flatten()
        .map(|v| sum(v.ephemeral_1h_input_tokens, v.ephemeral_5m_input_tokens))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let written = actual(
        input.cache_creation_input_tokens.flatten(),
        breakdown,
        "usage.cache_creation_input_tokens",
    )
    .or_else(|e| {
        if input.cache_creation_input_tokens.flatten().is_none() && breakdown.is_none() {
            actual(
                None,
                facts.cache_creation_input_tokens,
                "usage.cache_creation_input_tokens",
            )
        } else {
            Err(e)
        }
    })?;

    let cached = actual(
        input.cache_read_input_tokens.flatten(),
        facts.cache_read_input_tokens,
        "usage.cache_read_input_tokens",
    )?;
    let thinking = actual(
        input
            .output_tokens_details
            .flatten()
            .map(|v| v.thinking_tokens),
        facts.thinking_tokens,
        "usage.thinking_tokens",
    )?;
    let input_count = sum(sum(input.input_tokens, written)?, cached)?;
    let output = count(input.output_tokens)?;

    for (present, field) in [
        (breakdown.is_some(), "cache_creation"),
        (input.fallback_credit.is_some(), "fallback_credit"),
        (input.inference_geo.is_some(), "inference_geo"),
        (input.iterations.is_some(), "iterations"),
        (input.server_tool_use.is_some(), "server_tool_use"),
        (input.service_tier.is_some(), "service_tier"),
        (input.speed.is_some(), "speed"),
    ] {
        if present {
            report.omitted(
                format!("usage.{field}"),
                "Gemini has no equivalent Claude usage detail",
            );
        }
    }
    Ok(g::UsageMetadata::builder()
        .prompt_token_count(input_count)
        .cached_content_token_count(cached)
        .candidates_token_count(output - thinking)
        .thoughts_token_count(thinking)
        .total_token_count(sum(input_count, output)?)
        .build())
}
pub(super) fn to_claude(
    input: g::UsageMetadata,
    facts: ClaudeGeminiUsageFacts,
    report: &mut Report,
) -> Result<c::Usage, TransformError> {
    let prompt = actual(input.prompt_token_count, None, "usage.prompt_token_count")?;
    let prompt = sum(prompt, input.tool_use_prompt_token_count.unwrap_or(0))?;
    let candidates = actual(
        input.candidates_token_count,
        None,
        "usage.candidates_token_count",
    )?;
    let total = actual(input.total_token_count, None, "usage.total_token_count")?;
    let output = total
        .checked_sub(prompt)
        .ok_or_else(|| TransformError::invalid_result("usage", "total smaller than components"))?;
    let thinking = output - candidates;
    actual(
        input.thoughts_token_count,
        Some(thinking),
        "usage.thoughts_token_count",
    )?;

    let cached = actual(
        input.cached_content_token_count,
        facts.cache_read_input_tokens,
        "usage.cached_content_token_count",
    )?;
    let written = actual(
        None,
        facts.cache_creation_input_tokens,
        "usage.cache_creation_input_tokens",
    )?;
    let uncached = prompt
        .checked_sub(sum(cached, written)?)
        .ok_or_else(|| TransformError::invalid_result("usage.cache", "cache exceeds input"))?;
    for (present, field) in [
        (
            input.prompt_tokens_details.is_some(),
            "prompt_tokens_details",
        ),
        (input.cache_tokens_details.is_some(), "cache_tokens_details"),
        (
            input.candidates_tokens_details.is_some(),
            "candidates_tokens_details",
        ),
        (
            input.tool_use_prompt_tokens_details.is_some(),
            "tool_use_prompt_tokens_details",
        ),
        (input.service_tier.is_some(), "service_tier"),
    ] {
        if present {
            report.omitted(
                format!("usage.{field}"),
                "Claude lacks equivalent Gemini usage details",
            );
        }
    }
    let mut out = c::Usage::builder(uncached, output).build();
    out.cache_creation_input_tokens = Some(Some(written));
    out.cache_read_input_tokens = Some(Some(cached));
    out.output_tokens_details = Some(Some(c::OutputTokensDetails::builder(thinking).build()));
    Ok(out)
}
