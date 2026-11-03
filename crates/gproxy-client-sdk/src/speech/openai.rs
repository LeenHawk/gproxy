use super::*;
use crate::media_support::{compatibility, count, warn};
use protocol::openai::audio as o;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiSpeechOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub custom_voice: Option<o::CustomVoice>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(
    request: &SpeechRequest,
    streaming: bool,
) -> Result<Rendered<o::CreateSpeechRequestBody>> {
    let mut warnings = Vec::new();
    let options = &request.provider_options.openai;
    // upstream_docs/openai/docs/Create speech.md:22: maximum 4096 characters.
    if request.text.chars().count() > 4096 {
        return Err(Error::invalid("OpenAI speech text exceeds 4096 characters"));
    }
    let voice = if let Some(custom) = &options.custom_voice {
        if request.voice.is_some() {
            compatibility(
                &mut warnings,
                "voice",
                "OpenAI custom_voice overrides the portable voice",
            );
        }
        o::SpeechVoice::Custom(custom.clone())
    } else {
        o::SpeechVoice::Name(request.voice.clone().ok_or_else(|| {
            Error::invalid("OpenAI speech requires a voice or native custom_voice")
        })?)
    };
    let (format, _) = format(&request.format, &mut warnings);
    let mut body =
        o::CreateSpeechRequestBody::builder(request.model.id.clone(), request.text.clone(), voice)
            .rest(options.rest.clone())
            .build();
    body.instructions = request.instructions.clone();
    body.response_format = Some(format);
    body.stream_format = Some(if streaming {
        o::SpeechStreamFormat::Sse
    } else {
        o::SpeechStreamFormat::Audio
    });
    body.speed = request
        .speed
        .map(|speed| {
            serde_json::Number::from_f64(speed)
                .ok_or_else(|| Error::invalid("speech speed must be finite"))
        })
        .transpose()?;
    if request.language.is_some() {
        warn(
            &mut warnings,
            "language",
            "OpenAI CreateSpeech has no language parameter",
        );
    }
    if request.provider_options.gemini != gemini::GeminiSpeechOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.gemini",
            "Gemini speech options are omitted by OpenAI",
        );
    }
    Ok(Rendered {
        request: body,
        warnings,
    })
}
pub(super) fn format(
    value: &Option<AudioFormat>,
    warnings: &mut Vec<Warning>,
) -> (o::AudioResponseFormat, AudioFormat) {
    use o::AudioResponseFormat as F;
    let Some(value) = value else {
        // CreateSpeech's documented default response_format.
        return (
            F::Mp3,
            AudioFormat::Encoded {
                media_type: "audio/mpeg".into(),
            },
        );
    };
    let native = match value {
        AudioFormat::Encoded { media_type } => match media_type.as_str() {
            "audio/mpeg" | "audio/mp3" => F::Mp3,
            "audio/opus" | "audio/ogg" => F::Opus,
            "audio/aac" => F::Aac,
            "audio/flac" => F::Flac,
            "audio/wav" | "audio/x-wav" => F::Wav,
            _ => {
                warn(
                    warnings,
                    "format",
                    "Unsupported OpenAI speech encoding uses MP3",
                );
                F::Mp3
            }
        },
        AudioFormat::Pcm {
            sample_rate,
            channels,
            ..
        } => {
            if *sample_rate != 24000 || *channels != 1 {
                compatibility(
                    warnings,
                    "format",
                    "OpenAI raw PCM is 24 kHz mono; requested rate/channels are not available",
                );
            }
            F::Pcm
        }
        AudioFormat::Unknown(_) => {
            warn(warnings, "format", "Unknown speech format uses MP3");
            F::Mp3
        }
    };
    let effective = match native {
        F::Mp3 => encoded("audio/mpeg"),
        F::Opus => encoded("audio/ogg"),
        F::Aac => encoded("audio/aac"),
        F::Flac => encoded("audio/flac"),
        F::Wav => encoded("audio/wav"),
        F::Pcm => AudioFormat::pcm(24000, 1),
    };
    (native, effective)
}
fn encoded(mime: &str) -> AudioFormat {
    AudioFormat::Encoded {
        media_type: mime.into(),
    }
}
pub(super) fn usage(value: &o::SpeechUsage) -> Result<OperationUsage> {
    #[derive(Deserialize, Default)]
    struct Extra {
        characters: Option<u64>,
    }
    let extra: Extra = serde_json::from_value(serde_json::Value::Object(value.rest.clone()))
        .map_err(Error::invalid)?;
    Ok(OperationUsage {
        tokens: Usage {
            input_tokens: Some(count(value.input_tokens)?),
            output_tokens: Some(count(value.output_tokens)?),
            ..Default::default()
        },
        total_tokens: Some(count(value.total_tokens)?),
        characters: extra.characters,
        ..Default::default()
    })
}
