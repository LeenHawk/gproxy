use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

use super::{AudioTokenUsage, TranscriptionLanguage, TranscriptionLogprob};

/// Speech SSE payload observed by compatible backends. The OpenAI snapshot
/// confirms SSE transport but does not name its events; `type`, `delta`, and
/// `audio` are session-derived aliases and every other field remains opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SpeechStreamEvent {
    Event(SpeechEvent),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TranscriptionStreamEvent {
    Delta(TranscriptionTextDeltaEvent),
    Done(TranscriptionTextDoneEvent),
    Segment(TranscriptionTextSegmentEvent),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionTextDeltaEvent {
    #[serde(rename = "type")]
    pub type_: TranscriptionTextDeltaType,
    pub delta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TranscriptionLogprob>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segment_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionTextDeltaType {
    #[serde(rename = "transcript.text.delta")]
    Delta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionTextDoneEvent {
    #[serde(rename = "type")]
    pub type_: TranscriptionTextDoneType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<TranscriptionLanguage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TranscriptionLogprob>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AudioTokenUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionTextDoneType {
    #[serde(rename = "transcript.text.done")]
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionTextSegmentEvent {
    #[serde(rename = "type")]
    pub type_: TranscriptionTextSegmentType,
    pub id: String,
    pub end: f64,
    pub speaker: String,
    pub start: f64,
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionTextSegmentType {
    #[serde(rename = "transcript.text.segment")]
    Segment,
}
