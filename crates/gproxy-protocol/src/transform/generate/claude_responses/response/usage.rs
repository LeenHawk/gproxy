use crate::{
    transform::{Report, TransformError},
    wire::{claude::generate_content as c, openai::responses as r},
};

/// Real usage facts only for counters absent from the Claude response.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ResponsesUsageFacts {
    pub cache_write_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
}
fn count(value: i64, _field: &str) -> Result<i64, TransformError> {
    Ok(value)
}
fn sum(a: i64, b: i64) -> Result<i64, TransformError> {
    a.checked_add(b)
        .ok_or_else(|| TransformError::invalid_result("usage", "token count overflow"))
}
fn actual(source: Option<i64>, supplied: Option<i64>, field: &str) -> Result<i64, TransformError> {
    count(
        source
            .or(supplied)
            .ok_or_else(|| TransformError::missing_metadata(field))?,
        field,
    )
}
pub(crate) fn to_responses(
    input: c::Usage,
    facts: ResponsesUsageFacts,
    report: &mut Report,
) -> Result<r::ResponseUsage, TransformError> {
    let uncached = count(input.input_tokens, "usage.input_tokens")?;
    let output = count(input.output_tokens, "usage.output_tokens")?;
    let breakdown = input
        .cache_creation
        .flatten()
        .map(|v| {
            sum(
                count(v.ephemeral_1h_input_tokens, "cache_creation.1h")?,
                count(v.ephemeral_5m_input_tokens, "cache_creation.5m")?,
            )
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let written = actual(
        input.cache_creation_input_tokens.flatten().or(breakdown),
        facts.cache_write_tokens,
        "usage.cache_creation_input_tokens",
    )?;

    let cached = actual(
        input.cache_read_input_tokens.flatten(),
        facts.cached_tokens,
        "usage.cache_read_input_tokens",
    )?;
    let thinking = actual(
        input
            .output_tokens_details
            .flatten()
            .map(|v| v.thinking_tokens),
        facts.reasoning_tokens,
        "usage.output_tokens_details.thinking_tokens",
    )?;

    let total_input = sum(sum(uncached, written)?, cached)?;
    for (present, name) in [
        (breakdown.is_some(), "cache_creation"),
        (input.fallback_credit.is_some(), "fallback_credit"),
        (input.inference_geo.is_some(), "inference_geo"),
        (input.iterations.is_some(), "iterations"),
        (input.server_tool_use.is_some(), "server_tool_use"),
        (input.speed.is_some(), "speed"),
    ] {
        if present {
            report.omitted(
                format!("usage.{name}"),
                "Responses has no equivalent usage detail",
            );
        }
    }
    Ok(r::ResponseUsage {
        input_tokens: total_input,
        input_tokens_details: r::ResponseInputTokensDetails {
            cache_write_tokens: written,
            cached_tokens: cached,
            rest: Default::default(),
        },
        output_tokens: output,
        output_tokens_details: r::ResponseOutputTokensDetails {
            reasoning_tokens: thinking,
            rest: Default::default(),
        },
        total_tokens: sum(total_input, output)?,
        rest: Default::default(),
    })
}
pub(crate) fn to_claude(input: r::ResponseUsage) -> Result<c::Usage, TransformError> {
    let total_input = count(input.input_tokens, "usage.input_tokens")?;
    let output = count(input.output_tokens, "usage.output_tokens")?;
    let cached = count(
        input.input_tokens_details.cached_tokens,
        "usage.cached_tokens",
    )?;
    let written = count(
        input.input_tokens_details.cache_write_tokens,
        "usage.cache_write_tokens",
    )?;
    let thinking = count(
        input.output_tokens_details.reasoning_tokens,
        "usage.reasoning_tokens",
    )?;

    Ok(c::Usage {
        input_tokens: total_input - cached - written,
        output_tokens: output,
        cache_creation_input_tokens: Some(Some(written)),
        cache_read_input_tokens: Some(Some(cached)),
        output_tokens_details: Some(Some(c::OutputTokensDetails {
            thinking_tokens: thinking,
            rest: Default::default(),
        })),
        cache_creation: None,
        fallback_credit: None,
        inference_geo: None,
        iterations: None,
        server_tool_use: None,
        service_tier: None,
        speed: None,
        rest: Default::default(),
    })
}
