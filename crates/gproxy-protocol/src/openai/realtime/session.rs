use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{OpenAiModelId, ResponseToolChoice, Rest};
use crate::openai::generate_content::responses::ResponseTool;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeSession {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<RealtimeSessionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<RealtimeOutputModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<RealtimeAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponseToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<RealtimeMaxTokens>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RealtimeMaxTokens {
    Count(u64),
    Infinite(String),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeAudio {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<RealtimeAudioInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<RealtimeAudioOutput>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeAudioInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<RealtimeAudioFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_reduction: Option<RealtimeNoiseReduction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<RealtimeTranscription>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_detection: Option<RealtimeTurnDetectionSetting>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeAudioOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<RealtimeAudioFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RealtimeAudioFormat {
    Pcm(RealtimePcmFormat),
    Pcmu(RealtimePcmuFormat),
    Pcma(RealtimePcmaFormat),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimePcmFormat {
    #[serde(rename = "type")]
    pub type_: RealtimePcmFormatType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<u32>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealtimePcmFormatType {
    #[serde(rename = "audio/pcm")]
    Pcm,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimePcmuFormat {
    #[serde(rename = "type")]
    pub type_: RealtimePcmuFormatType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealtimePcmuFormatType {
    #[serde(rename = "audio/pcmu")]
    Pcmu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimePcmaFormat {
    #[serde(rename = "type")]
    pub type_: RealtimePcmaFormatType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealtimePcmaFormatType {
    #[serde(rename = "audio/pcma")]
    Pcma,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeNoiseReduction {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<RealtimeNoiseReductionType>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeTranscription {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RealtimeTurnDetectionSetting {
    Disabled(()),
    Vad(RealtimeTurnDetection),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RealtimeTurnDetection {
    Server(RealtimeServerVad),
    Semantic(RealtimeSemanticVad),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeServerVad {
    #[serde(rename = "type")]
    pub type_: RealtimeServerVadType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_padding_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_response: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt_response: Option<bool>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealtimeServerVadType {
    #[serde(rename = "server_vad")]
    ServerVad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeSemanticVad {
    #[serde(rename = "type")]
    pub type_: RealtimeSemanticVadType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eagerness: Option<RealtimeVadEagerness>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_response: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupt_response: Option<bool>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RealtimeSemanticVadType {
    #[serde(rename = "semantic_vad")]
    SemanticVad,
}

macro_rules! extensible_string {
    ($name:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $name { Known($known), Unknown(String) }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known { $(#[serde(rename = $wire)] $variant),+ }
    };
}

extensible_string!(RealtimeSessionType, KnownRealtimeSessionType {
    Realtime => "realtime", Transcription => "transcription",
});
extensible_string!(RealtimeOutputModality, KnownRealtimeOutputModality {
    Text => "text", Audio => "audio",
});
extensible_string!(RealtimeNoiseReductionType, KnownRealtimeNoiseReductionType {
    NearField => "near_field", FarField => "far_field",
});
extensible_string!(RealtimeVadEagerness, KnownRealtimeVadEagerness {
    Low => "low", Medium => "medium", High => "high", Auto => "auto",
});
