use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::ResponseToolChoice;
use super::super::common::*;
use super::super::generate_content::ResponseTool;
use super::realtime_string_enum;

realtime_string_enum!(RealtimeSessionType, RealtimeSessionTypeKnown {
    Realtime => "realtime",
    Transcription => "transcription",
});

realtime_string_enum!(RealtimeOutputModality, RealtimeOutputModalityKnown {
    Text => "text",
    Audio => "audio",
});

realtime_string_enum!(NoiseReductionType, NoiseReductionTypeKnown {
    NearField => "near_field",
    FarField => "far_field",
});

realtime_string_enum!(SemanticVadEagerness, SemanticVadEagernessKnown {
    Low => "low",
    Medium => "medium",
    High => "high",
    Auto => "auto",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RealtimeSessionConfig {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<RealtimeSessionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<RealtimeOutputModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<RealtimeAudioConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponseToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<RealtimeMaxTokens>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RealtimeMaxTokens {
    Count(u64),
    /// The wire literal `"inf"`.
    Infinite(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RealtimeAudioConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<RealtimeAudioInputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<RealtimeAudioOutputConfig>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RealtimeAudioInputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<RealtimeAudioFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_reduction: Option<NoiseReduction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<RealtimeTranscriptionConfig>,
    /// Omitted = server default; `Some(Disabled)` serializes the explicit
    /// `null` that turns server VAD off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_detection: Option<RealtimeTurnDetectionSetting>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RealtimeAudioOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<RealtimeAudioFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum RealtimeAudioFormat {
    #[serde(rename = "audio/pcm")]
    Pcm {
        #[serde(skip_serializing_if = "Option::is_none")]
        rate: Option<u32>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "audio/pcmu")]
    Pcmu {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "audio/pcma")]
    Pcma {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct NoiseReduction {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<NoiseReductionType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RealtimeTranscriptionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum RealtimeTurnDetectionSetting {
    /// Serializes as `null`: disables automatic turn detection.
    Disabled,
    Vad(RealtimeTurnDetection),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum RealtimeTurnDetection {
    #[serde(rename = "server_vad")]
    ServerVad {
        #[serde(skip_serializing_if = "Option::is_none")]
        threshold: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        prefix_padding_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        silence_duration_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        idle_timeout_ms: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        create_response: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt_response: Option<bool>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "semantic_vad")]
    SemanticVad {
        #[serde(skip_serializing_if = "Option::is_none")]
        eagerness: Option<SemanticVadEagerness>,
        #[serde(skip_serializing_if = "Option::is_none")]
        create_response: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupt_response: Option<bool>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}
