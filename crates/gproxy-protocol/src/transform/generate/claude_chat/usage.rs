use crate::{
    transform::{Report, TransformError, TransformErrorKind},
    wire::{claude::generate_content as cg, openai::chat},
};

fn count(value: i64) -> Result<i64, TransformError> {
    if value < 0 {
        return Err(TransformError::invalid_result(
            "usage",
            "negative token count",
        ));
    }
    Ok(value)
}
fn add(left: i64, right: i64) -> Result<i64, TransformError> {
    count(left)?
        .checked_add(count(right)?)
        .ok_or_else(|| TransformError::invalid_result("usage", "token count overflow"))
}
fn optional(value: Option<i64>) -> Result<Option<i64>, TransformError> {
    value.map(count).transpose()
}

pub(super) fn to_chat(
    input: &cg::Usage,
    report: &mut Report,
) -> Result<chat::Usage, TransformError> {
    let read = optional(input.cache_read_input_tokens.flatten())?;
    let explicit = optional(input.cache_creation_input_tokens.flatten())?;
    let breakdown = input
        .cache_creation
        .as_ref()
        .and_then(Option::as_ref)
        .map(|cache| {
            add(
                cache.ephemeral_1h_input_tokens,
                cache.ephemeral_5m_input_tokens,
            )
        })
        .transpose()?;
    if let (Some(explicit), Some(breakdown)) = (explicit, breakdown)
        && explicit != breakdown
    {
        return Err(TransformError::new(
            TransformErrorKind::Conflict,
            "usage.cache_creation",
            "cache total and breakdown disagree",
        ));
    }
    let write = explicit.or(breakdown);
    let prompt = add(
        add(input.input_tokens, read.unwrap_or(0))?,
        write.unwrap_or(0),
    )?;
    let total = add(prompt, input.output_tokens)?;
    let mut output = chat::Usage::builder(prompt, input.output_tokens, total).build();
    if read.is_some() || write.is_some() {
        let mut details = chat::PromptTokensDetails::builder().build();
        details.cached_tokens = read;
        details.cache_write_tokens = write;
        output.prompt_tokens_details = Some(details);
    }
    if let Some(details) = input
        .output_tokens_details
        .as_ref()
        .and_then(Option::as_ref)
    {
        if count(details.thinking_tokens)? > input.output_tokens {
            return Err(TransformError::invalid_result(
                "usage.thinking_tokens",
                "thinking tokens exceed output tokens",
            ));
        }
        let mut target = chat::CompletionTokensDetails::builder().build();
        target.reasoning_tokens = Some(details.thinking_tokens);
        output.completion_tokens_details = Some(target);
    }
    for (present, field) in [
        (input.cache_creation.is_some(), "cache_creation"),
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
                "Chat usage has no equivalent detail",
            );
        }
    }
    Ok(output)
}

pub(super) fn to_claude(
    input: &chat::Usage,
    report: &mut Report,
) -> Result<cg::Usage, TransformError> {
    let total = add(input.prompt_tokens, input.completion_tokens)?;
    if count(input.total_tokens)? != total {
        return Err(TransformError::invalid_result(
            "usage.total_tokens",
            "total differs from prompt plus completion tokens",
        ));
    }
    let details = input.prompt_tokens_details.as_ref();
    let read = optional(details.and_then(|details| details.cached_tokens))?;
    let write = optional(details.and_then(|details| details.cache_write_tokens))?;
    let cached = add(read.unwrap_or(0), write.unwrap_or(0))?;
    let uncached = input
        .prompt_tokens
        .checked_sub(cached)
        .filter(|value| *value >= 0)
        .ok_or_else(|| {
            TransformError::invalid_result(
                "usage.prompt_tokens",
                "cache token counts exceed prompt tokens",
            )
        })?;
    let mut output = cg::Usage::builder(uncached, input.completion_tokens).build();
    output.cache_read_input_tokens = read.map(Some);
    output.cache_creation_input_tokens = write.map(Some);
    if let Some(tokens) = input
        .completion_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens)
    {
        if count(tokens)? > input.completion_tokens {
            return Err(TransformError::invalid_result(
                "usage.reasoning_tokens",
                "reasoning tokens exceed completion tokens",
            ));
        }
        output.output_tokens_details = Some(Some(cg::OutputTokensDetails::builder(tokens).build()));
    }
    if details.is_some_and(|details| details.audio_tokens.is_some()) {
        report.omitted(
            "usage.prompt_tokens_details.audio_tokens",
            "Claude usage has no audio-token detail",
        );
    }
    if input
        .completion_tokens_details
        .as_ref()
        .is_some_and(|details| {
            details.audio_tokens.is_some()
                || details.accepted_prediction_tokens.is_some()
                || details.rejected_prediction_tokens.is_some()
        })
    {
        report.omitted(
            "usage.completion_tokens_details",
            "Claude usage has no audio or prediction-token details",
        );
    }
    Ok(output)
}
