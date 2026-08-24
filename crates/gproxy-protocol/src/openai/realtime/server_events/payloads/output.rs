use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::{RealtimeContentPart, RealtimeItem};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseOutputItemEvent {
    pub response_id: String,
    pub output_index: u32,
    pub item: RealtimeItem,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseContentPartEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub content_index: u32,
    pub part: RealtimeContentPart,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseOutputDeltaEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub content_index: u32,
    pub delta: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseOutputTextDoneEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub content_index: u32,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseAudioTranscriptDoneEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub content_index: u32,
    pub transcript: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseOutputAudioDoneEvent {
    pub response_id: String,
    pub item_id: String,
    pub output_index: u32,
    pub content_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}
