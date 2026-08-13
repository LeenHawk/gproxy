macro_rules! audio_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        #[serde(untagged)]
        #[non_exhaustive]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                crate::protocol::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[non_exhaustive]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant,)+
        }
    };
}

audio_string_enum!(SpeechResponseFormat, SpeechResponseFormatKnown {
    Mp3 => "mp3",
    Opus => "opus",
    Aac => "aac",
    Flac => "flac",
    Wav => "wav",
    Pcm => "pcm",
});

audio_string_enum!(SpeechStreamFormat, SpeechStreamFormatKnown {
    Sse => "sse",
    Audio => "audio",
});

audio_string_enum!(TranscriptionResponseFormat, TranscriptionResponseFormatKnown {
    Json => "json",
    Text => "text",
    Srt => "srt",
    VerboseJson => "verbose_json",
    Vtt => "vtt",
    DiarizedJson => "diarized_json",
});

audio_string_enum!(TranslationResponseFormat, TranslationResponseFormatKnown {
    Json => "json",
    Text => "text",
    Srt => "srt",
    VerboseJson => "verbose_json",
    Vtt => "vtt",
});

audio_string_enum!(TimestampGranularity, TimestampGranularityKnown {
    Word => "word",
    Segment => "segment",
});

audio_string_enum!(TranscriptionInclude, TranscriptionIncludeKnown {
    Logprobs => "logprobs",
});

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum AudioTokenUsageType {
    #[serde(rename = "tokens")]
    Tokens,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum AudioDurationUsageType {
    #[serde(rename = "duration")]
    Duration,
}
