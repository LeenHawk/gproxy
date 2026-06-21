use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{ExtraFields, SpeechLanguageCode};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeechConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub voice: Option<SpeechVoiceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<SpeechLanguageCode>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpeechVoiceConfig {
    VoiceConfig {
        #[serde(rename = "voiceConfig")]
        voice_config: VoiceConfig,
    },
    MultiSpeakerVoiceConfig {
        #[serde(rename = "multiSpeakerVoiceConfig")]
        multi_speaker_voice_config: MultiSpeakerVoiceConfig,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub voice_config: Option<VoiceConfigValue>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VoiceConfigValue {
    PrebuiltVoiceConfig {
        #[serde(rename = "prebuiltVoiceConfig")]
        prebuilt_voice_config: PrebuiltVoiceConfig,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrebuiltVoiceConfig {
    pub voice_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MultiSpeakerVoiceConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speaker_voice_configs: Vec<SpeakerVoiceConfig>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerVoiceConfig {
    pub speaker: String,
    pub voice_config: VoiceConfig,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}
