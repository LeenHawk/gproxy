use super::*;
use crate::media_support::{compatibility, warn};
use protocol::gemini as g;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiSpeechOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub speech_config: Option<g::SpeechConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub speech_metadata: Option<g::audio::SpeechMetadata>,
    #[cfg_attr(feature = "ts", ts(as = "Option<serde_json::Value>"))]
    pub response_format: Option<g::AudioResponseFormat>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(request: &SpeechRequest) -> Result<Rendered<g::GenerateContentRequestBody>> {
    let options = &request.provider_options.gemini;
    let mut warnings = Vec::new();
    let mut speech = options
        .speech_config
        .clone()
        .unwrap_or_else(|| g::SpeechConfig::builder().build());
    // Generating content.md:4407: single-voice and multi-speaker configs are exclusive.
    if speech.voice_config.is_some() && speech.multi_speaker_voice_config.is_some() {
        return Err(Error::invalid(
            "Gemini voice_config and multi_speaker_voice_config are mutually exclusive",
        ));
    }
    if let Some(voice) = &request.voice {
        if speech.voice_config.is_some() || speech.multi_speaker_voice_config.is_some() {
            compatibility(
                &mut warnings,
                "voice",
                "Native Gemini speech configuration overrides portable voice",
            );
        } else {
            speech.voice_config = Some(g::VoiceConfig::builder().voice(voice.clone()).build());
        }
    }
    if let Some(voice) = &speech.voice_config
        && voice.voice.is_some()
        && voice.prebuilt_voice_config.is_some()
    {
        return Err(Error::invalid(
            "Gemini voice and prebuilt_voice_config are mutually exclusive",
        ));
    }
    if request.language.is_some() {
        if speech.language_code.is_some() {
            compatibility(
                &mut warnings,
                "language",
                "Native Gemini speech language overrides portable language",
            );
        } else {
            speech.language_code = request.language.clone();
        }
    }
    let mut part = g::Part::builder().text(request.text.clone()).build();
    part.speech_metadata = options.speech_metadata.clone();
    if let Some(instructions) = &request.instructions {
        let meta = part
            .speech_metadata
            .get_or_insert_with(|| g::audio::SpeechMetadata::builder().build());
        if meta.style.is_some() {
            compatibility(
                &mut warnings,
                "instructions",
                "Native speech_metadata.style overrides portable instructions",
            );
        } else {
            meta.style = Some(instructions.clone());
        }
    }
    if request.speed.is_some() {
        warn(
            &mut warnings,
            "speed",
            "Gemini speech generation has no portable speed control",
        );
    }
    let format = if let Some(format) = &options.response_format {
        compatibility(
            &mut warnings,
            "format",
            "Native Gemini response_format overrides portable format",
        );
        format.clone()
    } else {
        format(&request.format, &mut warnings)
    };
    let mut config = g::GenerationConfig::builder()
        .response_modalities(vec![g::Modality::Audio])
        .speech_config(speech)
        .build();
    config.response_format = Some(g::ResponseFormatConfig::builder().audio(format).build());
    let body = g::GenerateContentRequestBody::builder(vec![
        g::Content::builder().role("user").parts(vec![part]).build(),
    ])
    .generation_config(config)
    .rest(options.rest.clone())
    .build();
    if request.provider_options.openai != openai::OpenaiSpeechOptions::default() {
        compatibility(
            &mut warnings,
            "provider_options.openai",
            "OpenAI speech options are omitted by Gemini",
        );
    }
    Ok(Rendered {
        request: body,
        warnings,
    })
}
fn format(value: &AudioFormat, warnings: &mut Vec<Warning>) -> g::AudioResponseFormat {
    use g::AudioMimeType as M;
    let mut format = g::AudioResponseFormat::builder()
        .delivery(g::Delivery::Inline)
        .build();
    format.mime_type = Some(match value {
        AudioFormat::Encoded { media_type } => match media_type.as_str() {
            "audio/mpeg" | "audio/mp3" => M::AudioMp3,
            "audio/ogg" | "audio/opus" => M::AudioOggOpus,
            "audio/wav" | "audio/x-wav" => M::AudioWav,
            "audio/alaw" => M::AudioAlaw,
            "audio/mulaw" => M::AudioMulaw,
            _ => {
                warn(
                    warnings,
                    "format",
                    "Unsupported Gemini speech encoding uses WAV",
                );
                M::AudioWav
            }
        },
        AudioFormat::Pcm {
            sample_rate,
            channels,
            ..
        } => {
            format.sample_rate = Some(i64::from(*sample_rate));
            if *channels != 1 {
                compatibility(warnings, "format.channels", "Gemini speech PCM is mono");
            }
            M::AudioL16
        }
        AudioFormat::Unknown(_) => {
            warn(warnings, "format", "Unknown speech encoding uses WAV");
            M::AudioWav
        }
    });
    format
}
