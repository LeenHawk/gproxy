use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::chat as c},
};
fn count(value: i64) -> Result<i64, TransformError> {
    Ok(value)
}
fn add(a: i64, b: i64) -> Result<i64, TransformError> {
    count(a)?
        .checked_add(count(b)?)
        .ok_or_else(|| TransformError::invalid_result("usage", "token overflow"))
}
fn required(v: Option<i64>, field: &str) -> Result<i64, TransformError> {
    count(v.ok_or_else(|| TransformError::missing_metadata(field))?)
}
/// Modality entries carry individual facts. Missing entries/counts are not zero.
fn audio_count(
    details: &Option<Vec<g::ModalityTokenCount>>,
    _total: Option<i64>,
    field: &str,
    report: &mut Report,
) -> Result<Option<i64>, TransformError> {
    let Some(details) = details else {
        return Ok(None);
    };
    let mut audio = None;
    let mut omitted = false;
    for entry in details {
        if let Some(value) = entry.token_count {
            count(value)?;
        }
        if entry.modality == Some(g::Modality::Audio) {
            audio = entry.token_count;
            if audio.is_none() {
                omitted = true;
            }
        } else {
            omitted = true;
        }
    }
    if omitted {
        report.omitted(
            field,
            "only a known AUDIO count has an equivalent Chat usage field",
        );
    }
    Ok(audio)
}
fn audio_detail(value: i64) -> Vec<g::ModalityTokenCount> {
    vec![
        g::ModalityTokenCount::builder()
            .modality(g::Modality::Audio)
            .token_count(value)
            .build(),
    ]
}
pub(super) fn to_chat(
    source: &g::UsageMetadata,
    report: &mut Report,
) -> Result<c::Usage, TransformError> {
    let prompt = required(source.prompt_token_count, "usage.prompt_token_count")?;
    let candidates = source
        .candidates_token_count
        .map(count)
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let total = required(source.total_token_count, "usage.total_token_count")?;
    let prompt = add(prompt, source.tool_use_prompt_token_count.unwrap_or(0))?;
    let completion = total
        .checked_sub(prompt)
        .ok_or_else(|| TransformError::invalid_result("usage", "total is smaller than prompt"))?;

    let inferred = candidates.map(|n| completion - n);
    let recorded = source
        .thoughts_token_count
        .map(count)
        .map(crate::transform::optional)
        .transpose()?
        .flatten();

    let thinking = recorded.or(inferred);
    let prompt_audio = audio_count(
        &source.prompt_tokens_details,
        source.prompt_token_count,
        "usage.prompt_tokens_details",
        report,
    )?;
    let tool_audio = audio_count(
        &source.tool_use_prompt_tokens_details,
        source.tool_use_prompt_token_count,
        "usage.tool_use_prompt_tokens_details",
        report,
    )?;
    let audio = if source.tool_use_prompt_token_count.unwrap_or(0) == 0 {
        prompt_audio
    } else if source.prompt_token_count == Some(0) {
        tool_audio
    } else if let (Some(a), Some(b)) = (prompt_audio, tool_audio) {
        Some(add(a, b)?)
    } else {
        if prompt_audio.is_some() || tool_audio.is_some() {
            report.omitted(
                "usage.prompt_audio",
                "partial initial/tool audio counts cannot describe the combined Chat prompt",
            );
        }
        None
    };
    let output_audio = audio_count(
        &source.candidates_tokens_details,
        Some(
            candidates
                .or(thinking.map(|n| completion - n))
                .unwrap_or(completion),
        ),
        "usage.candidates_tokens_details",
        report,
    )?;
    let mut out = c::Usage::builder(prompt, completion, total).build();
    if let Some(cached) = source.cached_content_token_count {
        out.prompt_tokens_details = Some(
            c::PromptTokensDetails::builder()
                .cached_tokens(cached)
                .build(),
        );
    }
    if let Some(audio) = audio {
        out.prompt_tokens_details
            .get_or_insert_with(|| c::PromptTokensDetails::builder().build())
            .audio_tokens = Some(audio);
    }
    out.completion_tokens_details = thinking.map(|thinking| {
        c::CompletionTokensDetails::builder()
            .reasoning_tokens(thinking)
            .build()
    });
    if let Some(audio) = output_audio {
        out.completion_tokens_details
            .get_or_insert_with(|| c::CompletionTokensDetails::builder().build())
            .audio_tokens = Some(audio);
    }
    for (present, field) in [
        (
            source.cache_tokens_details.is_some(),
            "cache_tokens_details",
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
    let thinking = source
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens);
    let candidates = thinking
        .map(|thinking| {
            source
                .completion_tokens
                .checked_sub(count(thinking)?)
                .ok_or_else(|| {
                    TransformError::invalid_result(
                        "usage.reasoning",
                        "reasoning exceeds completion",
                    )
                })
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if thinking.is_none() {
        report.omitted(
            "usage.reasoning_split",
            "Chat supplied no reasoning count; Gemini candidate/thought counts remain absent",
        );
    }
    let cached = source
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.cached_tokens);

    let mut out = g::UsageMetadata::builder().build();
    out.prompt_token_count = Some(source.prompt_tokens);
    out.candidates_token_count = candidates;
    out.total_token_count = Some(source.total_tokens);
    out.thoughts_token_count = thinking;
    out.cached_content_token_count = cached;
    if let Some(audio) = source
        .prompt_tokens_details
        .as_ref()
        .and_then(|d| d.audio_tokens)
    {
        out.prompt_tokens_details = Some(audio_detail(audio));
        report.changed(
            "usage.prompt_tokens_details",
            "preserved known AUDIO count; no other modality counts were invented",
        );
    }
    if let Some(audio) = source
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.audio_tokens)
    {
        out.candidates_tokens_details = Some(audio_detail(audio));
        report.changed(
            "usage.candidates_tokens_details",
            "preserved known AUDIO count; no other modality counts were invented",
        );
    }
    if source
        .prompt_tokens_details
        .as_ref()
        .is_some_and(|d| d.cache_write_tokens.is_some())
        || source.completion_tokens_details.as_ref().is_some_and(|d| {
            d.accepted_prediction_tokens.is_some() || d.rejected_prediction_tokens.is_some()
        })
    {
        report.omitted(
            "usage.details",
            "Gemini has no matching Chat prediction/cache-write detail",
        );
    }
    Ok(out)
}
