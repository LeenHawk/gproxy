use super::LiveContent;
use super::{LiveInt64, generation::LiveGenerationConfig, realtime::RealtimeInputConfig};
use crate::Rest;
use crate::gemini::Tool;
pub use crate::gemini::audio::{
    AudioTranscriptionConfig, AudioTranscriptionMode, LanguageAuto, LanguageHints,
};
use crate::gemini::content::SafetySetting;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentSetup {
    pub model: String,
    #[serde(alias = "generation_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<LiveGenerationConfig>,
    #[serde(alias = "system_instruction")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<LiveContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(alias = "realtime_input_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realtime_input_config: Option<RealtimeInputConfig>,
    #[serde(alias = "session_resumption")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_resumption: Option<SessionResumptionConfig>,
    #[serde(alias = "context_window_compression")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_compression: Option<ContextWindowCompressionConfig>,
    #[serde(alias = "input_audio_transcription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_audio_transcription: Option<AudioTranscriptionConfig>,
    #[serde(alias = "output_audio_transcription")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_audio_transcription: Option<AudioTranscriptionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proactivity: Option<ProactivityConfig>,
    #[serde(alias = "history_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_config: Option<HistoryConfig>,
    #[serde(alias = "avatar_config")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_config: Option<AvatarConfig>,
    #[serde(alias = "safety_settings")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SessionResumptionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContextWindowCompressionConfig {
    #[serde(alias = "sliding_window")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sliding_window: Option<SlidingWindow>,
    #[serde(alias = "trigger_tokens")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_tokens: Option<LiveInt64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SlidingWindow {
    #[serde(alias = "target_tokens")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_tokens: Option<LiveInt64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ProactivityConfig {
    #[serde(alias = "proactive_audio")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proactive_audio: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct HistoryConfig {
    #[serde(alias = "initial_history_in_client_content")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_history_in_client_content: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AvatarConfig {
    #[serde(alias = "avatar_name")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_name: Option<String>,
    #[serde(alias = "customized_avatar")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customized_avatar: Option<CustomizedAvatar>,
    #[serde(alias = "audio_bitrate_bps")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_bitrate_bps: Option<i64>,
    #[serde(alias = "video_bitrate_bps")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_bitrate_bps: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CustomizedAvatar {
    #[serde(alias = "image_mime_type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_mime_type: Option<String>,
    #[serde(alias = "image_data")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_data: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BidiGenerateContentSetupComplete {
    #[serde(alias = "session_id")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(alias = "voice_consent_signature")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_consent_signature: Option<VoiceConsentSignature>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct VoiceConsentSignature {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
