use crate::{
    transform::{Report, TransformError},
    wire::openai::{chat::response as c, responses::response as r},
};

/// Actual usage facts absent from an older Chat response. None never means zero.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChatUsageSupplement {
    pub cache_write_tokens: Option<i64>,
    pub cached_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
}

fn count(value: i64, field: &str) -> Result<i64, TransformError> {
    if value < 0 {
        return Err(TransformError::invalid_result(
            field,
            "negative token count",
        ));
    }
    Ok(value)
}
fn fact(source: Option<i64>, extra: Option<i64>, field: &str) -> Result<i64, TransformError> {
    if let (Some(a), Some(b)) = (source, extra)
        && a != b
    {
        return Err(TransformError::invalid_result(
            field,
            "supplied usage contradicts response",
        ));
    }
    count(
        source
            .or(extra)
            .ok_or_else(|| TransformError::missing_metadata(field))?,
        field,
    )
}
fn totals(input: i64, output: i64, total: i64) -> Result<(), TransformError> {
    count(input, "usage.input_tokens")?;
    count(output, "usage.output_tokens")?;
    if input.checked_add(output) != Some(total) {
        return Err(TransformError::invalid_result(
            "usage.total_tokens",
            "token total is inconsistent or overflows",
        ));
    }
    Ok(())
}
fn details(
    input: i64,
    output: i64,
    cached: i64,
    written: i64,
    reasoning: i64,
) -> Result<(), TransformError> {
    for (value, field) in [
        (cached, "usage.cached_tokens"),
        (written, "usage.cache_write_tokens"),
        (reasoning, "usage.reasoning_tokens"),
    ] {
        count(value, field)?;
    }
    if cached.checked_add(written).is_none_or(|n| n > input) || reasoning > output {
        return Err(TransformError::invalid_result(
            "usage",
            "token details exceed their totals",
        ));
    }
    Ok(())
}
pub(super) fn to_responses(
    source: c::Usage,
    extra: ChatUsageSupplement,
    report: &mut Report,
) -> Result<r::ResponseUsage, TransformError> {
    totals(
        source.prompt_tokens,
        source.completion_tokens,
        source.total_tokens,
    )?;
    let prompt = source.prompt_tokens_details;
    let completion = source.completion_tokens_details;
    let cached = fact(
        prompt.as_ref().and_then(|d| d.cached_tokens),
        extra.cached_tokens,
        "usage.cached_tokens",
    )?;
    let written = fact(
        prompt.as_ref().and_then(|d| d.cache_write_tokens),
        extra.cache_write_tokens,
        "usage.cache_write_tokens",
    )?;
    let reasoning = fact(
        completion.as_ref().and_then(|d| d.reasoning_tokens),
        extra.reasoning_tokens,
        "usage.reasoning_tokens",
    )?;
    details(
        source.prompt_tokens,
        source.completion_tokens,
        cached,
        written,
        reasoning,
    )?;
    for (present, field) in [
        (
            prompt.as_ref().is_some_and(|d| d.audio_tokens.is_some()),
            "usage.prompt_tokens_details.audio_tokens",
        ),
        (
            completion
                .as_ref()
                .is_some_and(|d| d.audio_tokens.is_some()),
            "usage.completion_tokens_details.audio_tokens",
        ),
        (
            completion
                .as_ref()
                .is_some_and(|d| d.accepted_prediction_tokens.is_some()),
            "usage.completion_tokens_details.accepted_prediction_tokens",
        ),
        (
            completion
                .as_ref()
                .is_some_and(|d| d.rejected_prediction_tokens.is_some()),
            "usage.completion_tokens_details.rejected_prediction_tokens",
        ),
    ] {
        if present {
            report.omitted(field, "Responses usage has no matching detail field");
        }
    }
    Ok(r::ResponseUsage {
        input_tokens: source.prompt_tokens,
        output_tokens: source.completion_tokens,
        total_tokens: source.total_tokens,
        input_tokens_details: r::ResponseInputTokensDetails {
            cached_tokens: cached,
            cache_write_tokens: written,
            rest: Default::default(),
        },
        output_tokens_details: r::ResponseOutputTokensDetails {
            reasoning_tokens: reasoning,
            rest: Default::default(),
        },
        rest: Default::default(),
    })
}
pub(super) fn to_chat(source: r::ResponseUsage) -> Result<c::Usage, TransformError> {
    totals(
        source.input_tokens,
        source.output_tokens,
        source.total_tokens,
    )?;
    details(
        source.input_tokens,
        source.output_tokens,
        source.input_tokens_details.cached_tokens,
        source.input_tokens_details.cache_write_tokens,
        source.output_tokens_details.reasoning_tokens,
    )?;
    Ok(c::Usage {
        prompt_tokens: source.input_tokens,
        completion_tokens: source.output_tokens,
        total_tokens: source.total_tokens,
        prompt_tokens_details: Some(c::PromptTokensDetails {
            cache_write_tokens: Some(source.input_tokens_details.cache_write_tokens),
            cached_tokens: Some(source.input_tokens_details.cached_tokens),
            audio_tokens: None,
            rest: Default::default(),
        }),
        completion_tokens_details: Some(c::CompletionTokensDetails {
            reasoning_tokens: Some(source.output_tokens_details.reasoning_tokens),
            audio_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
            rest: Default::default(),
        }),
        rest: Default::default(),
    })
}
