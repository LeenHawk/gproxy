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

/// The 3.8 Flash migration guide removes these controls. Native requests retain
/// their wire fields; only newly converted requests are adjusted for this model.
pub(crate) fn for_model(
    request: &mut crate::gemini::GenerateContentRequestBody,
    model: &str,
    report: &mut Report,
) {
    if model.rsplit('/').next() != Some("gemini-3.8-flash") {
        return;
    }
    let Some(config) = &mut request.generation_config else {
        return;
    };
    for (present, field) in [
        (config.temperature.take().is_some(), "temperature"),
        (config.top_p.take().is_some(), "top_p"),
        (config.top_k.take().is_some(), "top_k"),
        (config.candidate_count.take().is_some(), "candidate_count"),
    ] {
        if present {
            report.omitted(
                field,
                "Gemini 3.8 Flash no longer supports this generation control",
            );
        }
    }
    if let Some(thinking) = &mut config.thinking_config {
        let budget = thinking.thinking_budget.take();
        if budget.is_some() {
            report.omitted(
                "thinking_budget",
                "Gemini 3.8 Flash uses thinking_level instead of a token budget",
            );
        }
        if thinking.thinking_level == Some(crate::gemini::ThinkingLevel::Minimal)
            || (thinking.thinking_level.is_none() && budget == Some(0))
        {
            thinking.thinking_level = Some(crate::gemini::ThinkingLevel::Low);
            report.changed(
                "thinking_level",
                "Gemini 3.8 Flash requires at least low thinking effort",
            );
        }
    }
}
