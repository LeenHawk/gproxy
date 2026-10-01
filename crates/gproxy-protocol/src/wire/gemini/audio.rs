//! Shared speech, translation and transcription settings from Generating content.md.
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AudioTranscriptionConfig {
    #[serde(alias = "language_codes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_codes: Option<Vec<String>>,
    #[serde(alias = "language_auto")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_auto: Option<LanguageAuto>,
    #[serde(alias = "language_hints")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_hints: Option<LanguageHints>,
    #[serde(alias = "custom_vocabulary")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_vocabulary: Option<Vec<String>>,
    #[serde(alias = "adaptation_phrases")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptation_phrases: Option<Vec<String>>,
    #[serde(alias = "word_timestamp")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_timestamp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diarization: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AudioTranscriptionMode>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LanguageAuto {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LanguageHints {
    #[serde(alias = "language_codes")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language_codes: Option<Vec<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AudioTranscriptionMode {
    #[serde(rename = "MODE_UNSPECIFIED")]
    ModeUnspecified,
    #[serde(rename = "VERBATIM")]
    Verbatim,
    #[serde(rename = "SMART")]
    Smart,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
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
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SpeechMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct AudioTranscription {
    pub text: String,
    #[serde(alias = "speaker_label")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<WordInfo>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WordInfo {
    pub word: String,
    #[serde(alias = "start_offset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_offset: Option<String>,
    #[serde(alias = "end_offset")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_offset: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
