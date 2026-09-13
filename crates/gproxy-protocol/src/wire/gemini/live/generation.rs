use super::setup::{AudioTranscriptionConfig, VoiceConsentSignature};
use crate::Rest;
use crate::gemini::{content::*, generation::*};
use serde::{Deserialize, Serialize};

// GenerationConfig fields retain their native shape; endpoint support constraints
// (e.g. responseSchema is unsupported by Live) are not runtime validators here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "stop_sequences")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_mime_type")]
    pub response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_schema")]
    pub response_schema: Option<Schema>,
    #[serde(
        rename = "_responseJsonSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub response_json_schema_internal: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_json_schema")]
    pub response_json_schema: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_modalities")]
    pub response_modalities: Option<Vec<Modality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "candidate_count")]
    pub candidate_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_output_tokens")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_p")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "top_k")]
    pub top_k: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "presence_penalty")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "frequency_penalty")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_logprobs")]
    pub response_logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "enable_enhanced_civic_answers")]
    pub enable_enhanced_civic_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "speech_config")]
    pub speech_config: Option<LiveSpeechConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thinking_config")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "image_config")]
    pub image_config: Option<ImageConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "media_resolution")]
    pub media_resolution: Option<GenerationMediaResolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "response_format")]
    pub response_format: Option<ResponseFormatConfig>,
    #[serde(
        alias = "enable_affective_dialog",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_affective_dialog: Option<bool>,
    #[serde(
        alias = "translation_config",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub translation_config: Option<TranslationConfig>,
    #[serde(
        alias = "audio_transcription_config",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_transcription_config: Option<AudioTranscriptionConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveSpeechConfig {
    #[serde(alias = "voice_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_config: Option<LiveVoiceConfig>,
    #[serde(alias = "multi_speaker_voice_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_speaker_voice_config: Option<LiveMultiSpeakerVoiceConfig>,
    #[serde(alias = "language_code")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveVoiceConfig {
    #[serde(alias = "prebuilt_voice_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prebuilt_voice_config: Option<LivePrebuiltVoiceConfig>,
    #[serde(alias = "replicated_voice_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicated_voice_config: Option<ReplicatedVoiceConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LivePrebuiltVoiceConfig {
    #[serde(alias = "voice_name")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveMultiSpeakerVoiceConfig {
    #[serde(alias = "speaker_voice_configs")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_voice_configs: Option<Vec<LiveSpeakerVoiceConfig>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveSpeakerVoiceConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(alias = "voice_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_config: Option<LiveVoiceConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ReplicatedVoiceConfig {
    #[serde(alias = "mime_type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(alias = "voice_sample_audio")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_sample_audio: Option<String>,
    #[serde(alias = "consent_audio")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consent_audio: Option<String>,
    #[serde(alias = "voice_consent_signature")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_consent_signature: Option<VoiceConsentSignature>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct TranslationConfig {
    #[serde(alias = "echo_target_language")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echo_target_language: Option<bool>,
    #[serde(alias = "target_language_code")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_language_code: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
