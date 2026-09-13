use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::chat as c},
};
fn count(value: i64) -> Result<i64, TransformError> {
    if value < 0 {
        Err(TransformError::invalid_result(
            "usage",
            "negative token count",
        ))
    } else {
        Ok(value)
    }
}
fn add(a: i64, b: i64) -> Result<i64, TransformError> {
    count(a)?
        .checked_add(count(b)?)
        .ok_or_else(|| TransformError::invalid_result("usage", "token overflow"))
}
fn required(v: Option<i64>, field: &str) -> Result<i64, TransformError> {
    count(v.ok_or_else(|| TransformError::missing_metadata(field))?)
}
pub(super) fn to_chat(
    source: &g::UsageMetadata,
    report: &mut Report,
) -> Result<c::Usage, TransformError> {
    let prompt = required(source.prompt_token_count, "usage.prompt_token_count")?;
    let candidates = required(
        source.candidates_token_count,
        "usage.candidates_token_count",
    )?;
    let total = required(source.total_token_count, "usage.total_token_count")?;
    let prompt = add(prompt, source.tool_use_prompt_token_count.unwrap_or(0))?;
    let completion = total
        .checked_sub(prompt)
        .filter(|n| *n >= candidates)
        .ok_or_else(|| {
            TransformError::invalid_result("usage", "total is smaller than recorded counts")
        })?;
    let thinking = completion - candidates;
    if source.thoughts_token_count.is_some_and(|n| n != thinking) {
        return Err(TransformError::invalid_result(
            "usage.thoughts_token_count",
            "thinking count contradicts total",
        ));
    }
    let mut out = c::Usage::builder(prompt, completion, total).build();
    if let Some(cached) = source.cached_content_token_count {
        if count(cached)? > prompt {
            return Err(TransformError::invalid_result(
                "usage.cached",
                "cache exceeds prompt",
            ));
        }
        out.prompt_tokens_details = Some(
            c::PromptTokensDetails::builder()
                .cached_tokens(cached)
                .build(),
        );
    }
    out.completion_tokens_details = Some(
        c::CompletionTokensDetails::builder()
            .reasoning_tokens(thinking)
            .build(),
    );
    for (present, field) in [
        (
            source.prompt_tokens_details.is_some(),
            "prompt_tokens_details",
        ),
        (
            source.cache_tokens_details.is_some(),
            "cache_tokens_details",
        ),
        (
            source.candidates_tokens_details.is_some(),
            "candidates_tokens_details",
        ),
        (
            source.tool_use_prompt_tokens_details.is_some(),
            "tool_use_prompt_tokens_details",
        ),
        (source.service_tier.is_some(), "service_tier"),
    ] {
        if present {
            report.omitted(
                format!("usage.{field}"),
                "Chat has no equivalent Gemini usage detail",
            );
        }
    }
    Ok(out)
}
pub(super) fn to_gemini(
    source: &c::Usage,
    report: &mut Report,
) -> Result<g::UsageMetadata, TransformError> {
    if add(source.prompt_tokens, source.completion_tokens)? != source.total_tokens {
        return Err(TransformError::invalid_result(
            "usage.total",
            "inconsistent total",
        ));
    }
    let thinking = source
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens);
    let thinking = thinking.ok_or_else(|| {
        TransformError::missing_metadata("usage.completion_tokens_details.reasoning_tokens")
    })?;
    let candidates = source
        .completion_tokens
        .checked_sub(count(thinking)?)
        .filter(|v| *v >= 0)
        .ok_or_else(|| {
            TransformError::invalid_result("usage.reasoning", "reasoning exceeds completion")
        })?;
    let cached = source
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens);
    if let Some(cached) = cached
        && count(cached)? > source.prompt_tokens
    {
        return Err(TransformError::invalid_result(
            "usage.cached",
            "cache exceeds prompt",
        ));
    }
    let mut out = g::UsageMetadata::builder().build();
    out.prompt_token_count = Some(source.prompt_tokens);
    out.candidates_token_count = Some(candidates);
    out.total_token_count = Some(source.total_tokens);
    out.thoughts_token_count = Some(thinking);
    out.cached_content_token_count = cached;
    if source
        .prompt_tokens_details
        .as_ref()
        .is_some_and(|d| d.cache_write_tokens.is_some() || d.audio_tokens.is_some())
        || source.completion_tokens_details.as_ref().is_some_and(|d| {
            d.audio_tokens.is_some()
                || d.accepted_prediction_tokens.is_some()
                || d.rejected_prediction_tokens.is_some()
        })
    {
        report.omitted(
            "usage.details",
            "Gemini has no matching Chat audio/prediction/cache-write detail",
        );
    }
    Ok(out)
}
