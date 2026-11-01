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

/// Google's documented stand-in for a thought signature the model did not
/// issue (history from another model, or a client that dropped it).
/// https://ai.google.dev/gemini-api/docs/thought-signatures
pub(crate) const SKIP_THOUGHT_SIGNATURE: &str = "skip_thought_signature_validator";

/// The major version of a `gemini-<major>[.-]…` model id; `None` for any
/// other model (an Antigravity Claude model, Gemma).
fn gemini_generation(model: &str) -> Option<u32> {
    let rest = model.to_ascii_lowercase();
    let rest = rest.strip_prefix("gemini-")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    rest[..digits].parse().ok()
}

/// Gemini 3 refuses a model turn whose first function call has no thought
/// signature ("Function call is missing a thought_signature"); only the
/// first call of a step is signed. A converted model turn without any
/// signed call gets the documented stand-in on its first call.
fn skip_unsigned_calls(contents: &mut [crate::gemini::Content], report: &mut Report) {
    let mut changed = false;
    for content in contents
        .iter_mut()
        .filter(|content| content.role.as_deref() == Some("model"))
    {
        let Some(parts) = &mut content.parts else {
            continue;
        };
        let mut calls = parts.iter_mut().filter(|part| part.function_call.is_some());
        let Some(first) = calls.next() else {
            continue;
        };
        if first.thought_signature.is_some() {
            continue;
        }
        if calls.any(|part| part.thought_signature.is_some()) {
            continue;
        }
        first.thought_signature = Some(SKIP_THOUGHT_SIGNATURE.to_owned());
        changed = true;
    }
    if changed {
        report.changed(
            "thought_signature",
            "unsigned function call history marked with Gemini's skip_thought_signature_validator",
        );
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
    if gemini_generation(model).is_some_and(|major| major >= 3) {
        skip_unsigned_calls(&mut request.contents, report);
    }
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
