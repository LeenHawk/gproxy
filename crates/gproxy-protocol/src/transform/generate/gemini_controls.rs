//! Gemini model controls and metadata projection for cross-dialect conversions.
use crate::{gemini::Part, transform::Report};

pub(crate) fn omitted(part: &Part, report: &mut Report) {
    for (present, field) in [
        (part.media_processing.is_some(), "media_processing"),
        (part.audio_transcription.is_some(), "audio_transcription"),
        (part.speech_metadata.is_some(), "speech_metadata"),
        (
            part.inline_data
                .as_ref()
                .is_some_and(|v| v.display_name.is_some()),
            "inline_data.display_name",
        ),
        (
            part.file_data
                .as_ref()
                .is_some_and(|v| v.display_name.is_some()),
            "file_data.display_name",
        ),
    ] {
        if present {
            report.omitted(field, "Gemini media metadata has no target representation");
        }
    }
}

/// Migration controls apply only to newly converted requests. Native requests
/// retain their wire fields, including fields used by older models.
pub(crate) fn for_model(
    request: &mut crate::gemini::GenerateContentRequestBody,
    model: &str,
    report: &mut Report,
) {
    let model = model.rsplit('/').next().unwrap_or(model);
    let flash_38 = model == "gemini-3.8-flash";
    let level_only = flash_38 || model == "gemini-4-argon";
    if !level_only && model != "gemini-3.6-flash" {
        return;
    }
    let Some(config) = &mut request.generation_config else {
        return;
    };
    for (present, field) in [
        (config.temperature.take().is_some(), "temperature"),
        (config.top_p.take().is_some(), "top_p"),
        (config.top_k.take().is_some(), "top_k"),
        (
            flash_38 && config.candidate_count.take().is_some(),
            "candidate_count",
        ),
    ] {
        if present {
            report.omitted(
                field,
                "Target Gemini model no longer supports this generation control",
            );
        }
    }
    if !level_only {
        return;
    }
    if let Some(thinking) = &mut config.thinking_config {
        let budget = thinking.thinking_budget.take();
        if budget.is_some() {
            report.omitted(
                "thinking_budget",
                "Target Gemini model uses thinking_level instead of a token budget",
            );
        }
        if thinking.thinking_level == Some(crate::gemini::ThinkingLevel::Minimal)
            || (thinking.thinking_level.is_none() && budget == Some(0))
        {
            thinking.thinking_level = Some(crate::gemini::ThinkingLevel::Low);
            report.changed(
                "thinking_level",
                "Target Gemini model requires at least low thinking effort",
            );
        }
    }
}

/// Continuation carries opaque upstream state, not portable conversation history.
pub(crate) fn continuation_error() -> crate::transform::TransformError {
    crate::transform::TransformError::unsupported(
        "continuation_token",
        "Gemini long decode requires native continuation; cross-dialect continuation is unsupported",
    )
}
