use super::*;
use crate::media_support::{self as shared, assets, compatibility, count, io, metadata, warn};
use protocol::openai::audio as o;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiTranscriptionOptions {
    pub temperature: Option<f64>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub include: Option<Vec<o::TranscriptionInclude>>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub chunking_strategy: Option<o::ChunkingStrategy>,
    pub known_speaker_names: Option<Vec<String>>,
    pub known_speaker_references: Option<Vec<String>>,
    pub languages: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(
    request: &TranscriptionRequest,
    streaming: bool,
) -> Result<Rendered<o::CreateTranscriptionMultipartForm>> {
    let options = &request.provider_options.openai;
    let mut warnings = Vec::new();
    let audio = &request.audio.content;
    let mime = audio
        .format
        .media_type()
        .ok_or_else(|| Error::invalid("unknown transcription audio format"))?;
    assets::check(
        &audio.source,
        Some(mime),
        AssetDialect::OpenaiTranscription,
        MediaKind::Audio,
    )?;
    let Asset::Inline { bytes } = &audio.source else {
        return Err(Error::invalid(
            "OpenAI transcription requires inline upload bytes",
        ));
    };
    if matches!(audio.format, AudioFormat::Pcm { .. }) {
        return Err(Error::invalid(
            "OpenAI transcription needs an encoded audio container; no transcoding is performed",
        ));
    }
    let extension = match mime {
        "audio/flac" => "flac",
        "audio/mp3" | "audio/mpeg" => "mp3",
        "audio/mpga" => "mpga",
        "audio/mp4" | "video/mp4" => "mp4",
        "audio/m4a" => "m4a",
        "audio/ogg" => "ogg",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/webm" | "video/webm" => "webm",
        _ => return Err(Error::invalid("unsupported OpenAI transcription container")),
    };
    let file = shared::multipart::file(bytes.clone(), mime, &format!("audio.{extension}"))?;
    let diarized = request.diarization
        || options
            .known_speaker_names
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        || options
            .known_speaker_references
            .as_ref()
            .is_some_and(|s| !s.is_empty());
    let granularities = request
        .timestamps
        .iter()
        .filter_map(|g| match g {
            TimestampGranularity::Segment => Some(o::TranscriptionTimestampGranularity::Segment),
            TimestampGranularity::Word => Some(o::TranscriptionTimestampGranularity::Word),
            TimestampGranularity::Unknown(_) => {
                warn(
                    &mut warnings,
                    "timestamps",
                    "Unknown timestamp granularity is omitted",
                );
                None
            }
        })
        .collect::<Vec<_>>();
    let format = response_format(request);
    let mut body =
        o::CreateTranscriptionMultipartForm::builder(file, request.model.id.clone()).build();
    body.language = request.language.clone();
    body.prompt = request.prompt.clone();
    body.response_format = Some(format);
    body.languages = options.languages.clone();
    body.keywords = options.keywords.clone();
    body.temperature = options
        .temperature
        .map(|n| {
            serde_json::Number::from_f64(n)
                .ok_or_else(|| Error::invalid("transcription temperature must be finite"))
        })
        .transpose()?;
    body.chunking_strategy = options.chunking_strategy.clone().map(Some);
    body.known_speaker_names = options.known_speaker_names.clone();
    body.known_speaker_references = options.known_speaker_references.clone();
    if diarized {
        if request.timestamps.contains(&TimestampGranularity::Word) {
            warn(
                &mut warnings,
                "timestamps.word",
                "OpenAI diarized_json reports segment timestamps, not word timestamps",
            );
        }
        if request.prompt.is_some() {
            warn(
                &mut warnings,
                "prompt",
                "OpenAI diarized transcription does not support prompts",
            );
            body.prompt = None;
        }
    } else if !granularities.is_empty() {
        body.timestamp_granularities = Some(granularities);
    }
    if options.include.is_some() {
        if format == o::TranscriptionResponseFormat::Json {
            body.include = options.include.clone();
        } else {
            warn(
                &mut warnings,
                "provider_options.openai.include",
                "Logprobs are available with JSON transcription only",
            );
        }
    }
    // Verbose JSON has no matching streaming event contract; preserve timestamps.
    if streaming && format == o::TranscriptionResponseFormat::VerboseJson {
        compatibility(
            &mut warnings,
            "stream",
            "Verbose timestamp output is delivered as a completed transcription",
        );
    } else {
        body.stream = Some(Some(streaming));
    }
    if audio.transcript.is_some() {
        warn(
            &mut warnings,
            "audio.transcript",
            "An existing transcript is not submitted as audio; use prompt for hints",
        );
    }
    if request.audio.cache.is_some() {
        warn(
            &mut warnings,
            "audio.cache",
            "Transcription has no cache breakpoint",
        );
    }
    if !request.audio.provider_metadata.is_empty() {
        compatibility(
            &mut warnings,
            "audio.provider_metadata",
            "Transcription submits the audio source without conversation metadata",
        );
    }
    if request.provider_options.gemini != gemini::GeminiTranscriptionOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.gemini",
            "Gemini transcription options are omitted by OpenAI",
        );
    }
    Ok(Rendered {
        request: body,
        warnings,
    })
}
pub fn parse(data: &str, format: o::TranscriptionResponseFormat) -> Result<TranscriptionResult> {
    let mut result = TranscriptionResult::empty(Vec::new());
    result.finish_reason = io::stop();
    match format {
        o::TranscriptionResponseFormat::Json => {
            let native: o::TranscriptionJson = io::parse(data)?;
            result.text = native.text;
            result.usage = usage(native.usage.as_ref())?;
            let languages = native
                .languages
                .as_ref()
                .map(|l| l.iter().map(|l| l.code.clone()).collect::<Vec<_>>())
                .unwrap_or_default();
            if languages.len() == 1 {
                result.language = languages.first().cloned();
            } else if languages.len() > 1 {
                compatibility(
                    &mut result.warnings,
                    "response.language",
                    "Multiple languages were reported; the singular language is unset",
                );
            }
            #[derive(Serialize)]
            struct Extras {
                languages: Option<Vec<o::TranscriptionLanguage>>,
                logprobs: Option<Vec<o::TranscriptionLogprob>>,
                #[serde(flatten)]
                rest: protocol::Rest,
            }
            result.provider_metadata = [(
                Vendor::Openai,
                metadata(Extras {
                    languages: native.languages,
                    logprobs: native.logprobs,
                    rest: native.rest,
                })?,
            )]
            .into();
        }
        o::TranscriptionResponseFormat::VerboseJson => {
            let native: o::TranscriptionVerbose = io::parse(data)?;
            result.text = native.text;
            result.language = Some(native.language);
            result.duration = Some(number(native.duration)?);
            if let Some(u) = native.usage {
                result.usage.audio_seconds = Some(number(u.seconds)?);
            }
            for value in native.segments.into_iter().flatten() {
                result.segments.push(segment(value)?);
            }
            for value in native.words.into_iter().flatten() {
                result.words.push(TranscriptWord {
                    text: value.word,
                    start: Some(number(value.start)?),
                    end: Some(number(value.end)?),
                    speaker: None,
                    provider_metadata: [(Vendor::Openai, value.rest)].into(),
                });
            }
            result.provider_metadata = [(Vendor::Openai, native.rest)].into();
        }
        o::TranscriptionResponseFormat::DiarizedJson => {
            let native: o::TranscriptionDiarized = io::parse(data)?;
            result.text = native.text;
            result.duration = Some(number(native.duration)?);
            result.usage = usage(native.usage.as_ref())?;
            for value in native.segments {
                result.segments.push(diarized_segment(value)?);
            }
            result.provider_metadata = [(Vendor::Openai, native.rest)].into();
        }
        _ => {
            return Err(Error::invalid(
                "thin transcription expects JSON, verbose_json or diarized_json",
            ));
        }
    }
    Ok(result)
}
pub(super) fn number(value: serde_json::Number) -> Result<f64> {
    value
        .as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| Error::invalid("transcription value is not finite"))
}
fn segment(value: o::TranscriptionSegment) -> Result<TranscriptSegment> {
    let text = value.text;
    let start = number(value.start)?;
    let end = number(value.end)?;
    #[derive(Serialize)]
    struct Extra {
        id: i64,
        avg_logprob: serde_json::Number,
        compression_ratio: serde_json::Number,
        no_speech_prob: serde_json::Number,
        seek: i64,
        temperature: serde_json::Number,
        tokens: Vec<i64>,
        #[serde(flatten)]
        rest: protocol::Rest,
    }
    Ok(TranscriptSegment {
        text,
        start: Some(start),
        end: Some(end),
        speaker: None,
        provider_metadata: [(
            Vendor::Openai,
            metadata(Extra {
                id: value.id,
                avg_logprob: value.avg_logprob,
                compression_ratio: value.compression_ratio,
                no_speech_prob: value.no_speech_prob,
                seek: value.seek,
                temperature: value.temperature,
                tokens: value.tokens,
                rest: value.rest,
            })?,
        )]
        .into(),
    })
}
pub(super) fn diarized_segment(
    value: o::TranscriptionDiarizedSegment,
) -> Result<TranscriptSegment> {
    #[derive(Serialize)]
    struct Extra {
        id: String,
        #[serde(flatten)]
        rest: protocol::Rest,
    }
    Ok(TranscriptSegment {
        text: value.text,
        start: Some(number(value.start)?),
        end: Some(number(value.end)?),
        speaker: Some(value.speaker),
        provider_metadata: [(
            Vendor::Openai,
            metadata(Extra {
                id: value.id,
                rest: value.rest,
            })?,
        )]
        .into(),
    })
}
pub(super) fn token_usage(u: &o::TranscriptionTokenUsage) -> Result<OperationUsage> {
    Ok(OperationUsage {
        tokens: Usage {
            input_tokens: Some(count(u.input_tokens)?),
            output_tokens: Some(count(u.output_tokens)?),
            ..Default::default()
        },
        total_tokens: Some(count(u.total_tokens)?),
        ..Default::default()
    })
}
fn usage(value: Option<&o::TranscriptionUsage>) -> Result<OperationUsage> {
    match value {
        Some(o::TranscriptionUsage::Tokens(u)) => token_usage(u),
        Some(o::TranscriptionUsage::Duration(u)) => Ok(OperationUsage {
            audio_seconds: Some(number(u.seconds.clone())?),
            ..Default::default()
        }),
        None => Ok(OperationUsage::default()),
    }
}

pub(super) fn response_format(request: &TranscriptionRequest) -> o::TranscriptionResponseFormat {
    let options = &request.provider_options.openai;
    let diarized = request.diarization
        || options
            .known_speaker_names
            .as_ref()
            .is_some_and(|v| !v.is_empty())
        || options
            .known_speaker_references
            .as_ref()
            .is_some_and(|v| !v.is_empty());
    if diarized {
        o::TranscriptionResponseFormat::DiarizedJson
    } else if request.timestamps.iter().any(|g| {
        matches!(
            g,
            TimestampGranularity::Segment | TimestampGranularity::Word
        )
    }) {
        o::TranscriptionResponseFormat::VerboseJson
    } else {
        o::TranscriptionResponseFormat::Json
    }
}
