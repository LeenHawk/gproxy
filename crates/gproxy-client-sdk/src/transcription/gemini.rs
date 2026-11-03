use super::*;
use crate::media_support::{self as shared, assets, compatibility, warn};
use protocol::gemini as g;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiTranscriptionOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub config: Option<g::audio::AudioTranscriptionConfig>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(request: &TranscriptionRequest) -> Result<Rendered<g::GenerateContentRequestBody>> {
    let mut warnings = Vec::new();
    let options = &request.provider_options.gemini;
    let mut config = options
        .config
        .clone()
        .unwrap_or_else(|| g::audio::AudioTranscriptionConfig::builder().build());
    // Generating content.md:4662-4676: deprecated language_auto/hints form a union.
    if config.language_auto.is_some() && config.language_hints.is_some() {
        return Err(Error::invalid(
            "Gemini language_auto and language_hints are mutually exclusive",
        ));
    }
    if let Some(language) = &request.language {
        if config.language_codes.is_some()
            || config.language_auto.is_some()
            || config.language_hints.is_some()
        {
            compatibility(
                &mut warnings,
                "language",
                "Native Gemini language configuration overrides portable language",
            );
        } else {
            config.language_codes = Some(vec![language.clone()]);
        }
    }
    if config.language_auto.is_some() || config.language_hints.is_some() {
        warnings.push(Warning::new(
            WarningKind::Deprecated,
            "provider_options.gemini.config.language",
            "Gemini recommends top-level language_codes instead of deprecated language controls",
        ));
    }
    if config.adaptation_phrases.is_some() {
        warnings.push(Warning::new(
            WarningKind::Deprecated,
            "provider_options.gemini.config.adaptation_phrases",
            "Gemini recommends custom_vocabulary instead of adaptation_phrases",
        ));
    }
    let mut timestamps = false;
    for granularity in &request.timestamps {
        match granularity {
            TimestampGranularity::Word => timestamps = true,
            TimestampGranularity::Segment => {
                timestamps = true;
                compatibility(
                    &mut warnings,
                    "timestamps.segment",
                    "Gemini provides word offsets; segment bounds use the first/last reported word offsets",
                );
            }
            TimestampGranularity::Unknown(_) => warn(
                &mut warnings,
                "timestamps",
                "Unknown timestamp granularity is omitted",
            ),
        }
    }
    if timestamps {
        if config.word_timestamp == Some(false) {
            compatibility(
                &mut warnings,
                "timestamps",
                "Native Gemini word_timestamp overrides portable timestamps",
            );
        } else if config.word_timestamp.is_none() {
            config.word_timestamp = Some(true);
        }
    }
    if request.diarization {
        if config.diarization == Some(false) {
            compatibility(
                &mut warnings,
                "diarization",
                "Native Gemini diarization overrides the portable setting",
            );
        } else if config.diarization.is_none() {
            config.diarization = Some(true);
        }
    }
    let annotated = config.word_timestamp == Some(true) || config.diarization == Some(true);
    // Generating content.md:4661; Audio transcription.md:353-357,371-372.
    if annotated
        && (config.mode == Some(g::audio::AudioTranscriptionMode::Smart)
            || config
                .custom_vocabulary
                .as_ref()
                .is_some_and(|v| !v.is_empty()))
    {
        return Err(Error::invalid(
            "Gemini SMART/custom vocabulary cannot be combined with word timestamps or diarization",
        ));
    }
    let mime = shared::audio::pcm_mime(&request.audio.content.format)?;
    let audio = assets::part(
        &request.audio.content.source,
        Some(&mime),
        AssetDialect::GeminiTranscription,
        MediaKind::Audio,
    )?;
    let mut parts = Vec::new();
    if let Some(prompt) = &request.prompt {
        parts.push(g::Part::builder().text(prompt.clone()).build());
    }
    parts.push(audio);
    let generation = g::GenerationConfig::builder()
        .audio_transcription_config(config)
        .response_modalities(vec![g::Modality::Text])
        .build();
    let body = g::GenerateContentRequestBody::builder(vec![
        g::Content::builder().role("user").parts(parts).build(),
    ])
    .generation_config(generation)
    .rest(options.rest.clone())
    .build();
    if request.audio.content.transcript.is_some() {
        warn(
            &mut warnings,
            "audio.transcript",
            "An existing transcript is not used as the transcription result",
        );
    }
    if request.audio.cache.is_some() {
        warn(
            &mut warnings,
            "audio.cache",
            "Transcription has no per-block cache control",
        );
    }
    if !request.audio.provider_metadata.is_empty() {
        compatibility(
            &mut warnings,
            "audio.provider_metadata",
            "Transcription uses the audio source without conversation metadata",
        );
    }
    if request.provider_options.openai != openai::OpenaiTranscriptionOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.openai",
            "OpenAI transcription options are omitted by Gemini",
        );
    }
    Ok(Rendered {
        request: body,
        warnings,
    })
}
pub fn parse(response: g::GenerateContentResponseBody) -> Result<TranscriptionResult> {
    let mut result = TranscriptionResult::empty(Vec::new());
    apply(&mut result, response)?;
    Ok(result)
}
pub(super) fn apply(
    result: &mut TranscriptionResult,
    mut response: g::GenerateContentResponseBody,
) -> Result<Vec<TranscriptSegment>> {
    shared::gemini::candidate_warning(&response, &mut result.warnings);
    result.response.merge(shared::gemini::response(&response));
    if response.usage_metadata.is_some() {
        result
            .usage
            .observe(shared::gemini::usage(response.usage_metadata.as_ref())?);
    }
    let blocked = shared::gemini::blocked(&response);
    let mut reason = None;
    let mut observed = Vec::new();
    if let Some(candidate) = response.candidates.take().into_iter().flatten().next() {
        reason = candidate.finish_reason;
        for part in candidate
            .content
            .into_iter()
            .flat_map(|c| c.parts.into_iter().flatten())
        {
            let Some(value) = part.audio_transcription else {
                continue;
            };
            let mut words = Vec::new();
            for word in value.words.into_iter().flatten() {
                words.push(TranscriptWord {
                    text: word.word,
                    start: offset(word.start_offset)?,
                    end: offset(word.end_offset)?,
                    speaker: value.speaker_label.clone(),
                    provider_metadata: [(Vendor::Gemini, word.rest)].into(),
                });
            }
            let segment = TranscriptSegment {
                text: value.text,
                start: words.first().and_then(|w| w.start),
                end: words.last().and_then(|w| w.end),
                speaker: value.speaker_label,
                provider_metadata: [(Vendor::Gemini, value.rest)].into(),
            };
            result.text.push_str(&segment.text);
            result.words.extend(words);
            result.segments.push(segment.clone());
            observed.push(segment);
        }
    }
    if blocked || reason.is_some() {
        result.finish_reason = shared::gemini::finish(reason, response.prompt_feedback.as_ref())?;
    }
    if result.finish_reason.unified == FinishReasonKind::Stop && result.segments.is_empty() {
        return Err(Error::invalid(
            "Gemini completed without audio_transcription",
        ));
    }
    shared::gemini::observe_extras(
        result.provider_metadata.entry(Vendor::Gemini).or_default(),
        &response,
    )?;
    Ok(observed)
}
fn offset(value: Option<String>) -> Result<Option<f64>> {
    value
        .map(|value| {
            let number = value
                .strip_suffix('s')
                .ok_or_else(|| {
                    Error::invalid("Gemini timestamp offset must be a duration ending in s")
                })?
                .parse::<f64>()
                .map_err(Error::invalid)?;
            if number.is_finite() {
                Ok(number)
            } else {
                Err(Error::invalid("Gemini timestamp offset is not finite"))
            }
        })
        .transpose()
}
