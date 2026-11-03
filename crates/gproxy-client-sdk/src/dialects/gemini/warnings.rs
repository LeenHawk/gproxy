use crate::dialects::shared::warnings::Walk;
use crate::*;
pub(crate) fn for_request(walk: &mut Walk, request: &GenerateRequest) {
    let options = &request.provider_options;
    if walk.vendor == Vendor::Gemini
        && let Some(config) = &options.gemini.generation_config
    {
        for (ignored, field) in [
            (
                request.settings.temperature.is_some() && config.temperature.is_some(),
                "settings.temperature",
            ),
            (
                request.settings.top_p.is_some() && config.top_p.is_some(),
                "settings.top_p",
            ),
            (
                request.settings.max_output_tokens.is_some() && config.max_output_tokens.is_some(),
                "settings.max_output_tokens",
            ),
            (
                !request.settings.stop_sequences.is_empty() && config.stop_sequences.is_some(),
                "settings.stop_sequences",
            ),
            (
                !request.settings.output_modalities.is_empty()
                    && config.response_modalities.is_some(),
                "settings.output_modalities",
            ),
        ] {
            if ignored {
                walk.warn(
                    WarningKind::Compatibility,
                    field,
                    "Gemini generationConfig overrides the portable setting",
                );
            }
        }
        if request.settings.output_schema.is_some()
            && (config.response_schema.is_some()
                || config.response_json_schema.is_some()
                || config
                    .response_mime_type
                    .as_deref()
                    .is_some_and(|mime| mime != "application/json"))
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.output_schema",
                "Gemini response format options override the portable schema",
            );
        }
        if request.settings.speech.is_some()
            && config.speech_config.as_ref().is_some_and(|speech| {
                speech.multi_speaker_voice_config.is_some()
                    || speech
                        .voice_config
                        .as_ref()
                        .and_then(|voice| voice.prebuilt_voice_config.as_ref())
                        .and_then(|voice| voice.voice_name.as_ref())
                        .is_some()
            })
        {
            walk.warn(
                WarningKind::Compatibility,
                "settings.speech.voice",
                "Gemini speechConfig overrides the portable voice",
            );
        }
    }
}
