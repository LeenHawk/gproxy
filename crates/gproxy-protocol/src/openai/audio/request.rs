use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{OpenAiModelId, Rest, VoiceName};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechRequest {
    pub input: String,
    pub model: OpenAiModelId,
    pub voice: SpeechVoice,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<SpeechResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_format: Option<SpeechStreamFormat>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpeechVoice {
    Named(VoiceName),
    Custom(CustomVoice),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomVoice {
    pub id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionRequest {
    pub file: String,
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunking_strategy: Option<AudioChunkingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<TranscriptionInclude>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_speaker_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub known_speaker_references: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<TranscriptionResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp_granularities: Option<Vec<TimestampGranularity>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudioChunkingStrategy {
    Auto(AudioChunkingAuto),
    ServerVad(ServerVadConfig),
    Raw(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioChunkingAuto {
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerVadConfig {
    #[serde(rename = "type")]
    pub type_: ServerVadType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_padding_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerVadType {
    #[serde(rename = "server_vad")]
    ServerVad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub file: String,
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<TranslationResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

macro_rules! extensible_string {
    ($name:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $name {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant),+
        }
    };
}

extensible_string!(SpeechResponseFormat, KnownSpeechResponseFormat {
    Mp3 => "mp3", Opus => "opus", Aac => "aac", Flac => "flac", Wav => "wav", Pcm => "pcm",
});
extensible_string!(SpeechStreamFormat, KnownSpeechStreamFormat {
    Sse => "sse", Audio => "audio",
});
extensible_string!(TranscriptionResponseFormat, KnownTranscriptionResponseFormat {
    Json => "json", Text => "text", Srt => "srt", VerboseJson => "verbose_json",
    Vtt => "vtt", DiarizedJson => "diarized_json",
});
extensible_string!(TranslationResponseFormat, KnownTranslationResponseFormat {
    Json => "json", Text => "text", Srt => "srt", VerboseJson => "verbose_json", Vtt => "vtt",
});
extensible_string!(TimestampGranularity, KnownTimestampGranularity {
    Word => "word", Segment => "segment",
});
extensible_string!(TranscriptionInclude, KnownTranscriptionInclude {
    Logprobs => "logprobs",
});
