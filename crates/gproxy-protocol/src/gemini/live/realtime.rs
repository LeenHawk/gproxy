use crate::Rest;
use crate::gemini::generation::GenerationMediaResolution;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BidiGenerateContentRealtimeInput {
    #[serde(alias = "media_chunks")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_chunks: Option<Vec<LiveBlob>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<LiveBlob>,
    #[serde(alias = "audio_stream_end")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_stream_end: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<LiveBlob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(alias = "activity_start")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_start: Option<LiveActivityStart>,
    #[serde(alias = "activity_end")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_end: Option<LiveActivityEnd>,
    #[serde(alias = "media_resolution")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<GenerationMediaResolution>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveBlob {
    #[serde(alias = "mime_type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveActivityStart {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveActivityEnd {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeInputConfig {
    #[serde(alias = "automatic_activity_detection")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automatic_activity_detection: Option<AutomaticActivityDetection>,
    #[serde(alias = "activity_handling")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity_handling: Option<ActivityHandling>,
    #[serde(alias = "turn_coverage")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_coverage: Option<TurnCoverage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct AutomaticActivityDetection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(alias = "start_of_speech_sensitivity")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_of_speech_sensitivity: Option<StartSensitivity>,
    #[serde(alias = "prefix_padding_ms")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix_padding_ms: Option<i64>,
    #[serde(alias = "end_of_speech_sensitivity")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_of_speech_sensitivity: Option<EndSensitivity>,
    #[serde(alias = "silence_duration_ms")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ActivityHandling {
    #[serde(rename = "ACTIVITY_HANDLING_UNSPECIFIED")]
    ActivityHandlingUnspecified,
    #[serde(rename = "START_OF_ACTIVITY_INTERRUPTS")]
    StartOfActivityInterrupts,
    #[serde(rename = "NO_INTERRUPTION")]
    NoInterruption,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum TurnCoverage {
    #[serde(rename = "TURN_COVERAGE_UNSPECIFIED")]
    TurnCoverageUnspecified,
    #[serde(rename = "TURN_INCLUDES_ONLY_ACTIVITY")]
    TurnIncludesOnlyActivity,
    #[serde(rename = "TURN_INCLUDES_ALL_INPUT")]
    TurnIncludesAllInput,
    #[serde(rename = "TURN_INCLUDES_AUDIO_ACTIVITY_AND_ALL_VIDEO")]
    TurnIncludesAudioActivityAndAllVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum StartSensitivity {
    #[serde(rename = "START_SENSITIVITY_UNSPECIFIED")]
    StartSensitivityUnspecified,
    #[serde(rename = "START_SENSITIVITY_HIGH")]
    StartSensitivityHigh,
    #[serde(rename = "START_SENSITIVITY_LOW")]
    StartSensitivityLow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum EndSensitivity {
    #[serde(rename = "END_SENSITIVITY_UNSPECIFIED")]
    EndSensitivityUnspecified,
    #[serde(rename = "END_SENSITIVITY_HIGH")]
    EndSensitivityHigh,
    #[serde(rename = "END_SENSITIVITY_LOW")]
    EndSensitivityLow,
}
