use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use crate::openai::audio::AudioUsage;

use super::super::super::RealtimeError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputTranscriptionDeltaEvent {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    pub delta: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputTranscriptionCompletedEvent {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    pub transcript: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AudioUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputTranscriptionFailedEvent {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_index: Option<u32>,
    pub error: RealtimeError,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputTranscriptionSegmentEvent {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputAudioBufferCommittedEvent {
    pub item_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_item_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputAudioBufferClearedEvent {
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputAudioSpeechStartedEvent {
    pub item_id: String,
    pub audio_start_ms: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputAudioSpeechStoppedEvent {
    pub item_id: String,
    pub audio_end_ms: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputAudioTimeoutEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_start_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_end_ms: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeOutputAudioBufferEvent {
    pub response_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
