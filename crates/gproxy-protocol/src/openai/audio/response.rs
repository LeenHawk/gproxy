use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TranscriptionResponse {
    Verbose(TranscriptionVerbose),
    Diarized(TranscriptionDiarized),
    Json(Transcription),
    Text(String),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TranslationResponse {
    Verbose(TranslationVerbose),
    Json(Translation),
    Text(String),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcription {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<TranscriptionLanguage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<TranscriptionLogprob>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AudioUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionVerbose {
    pub duration: f64,
    pub language: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<TranscriptionSegment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AudioDurationUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub words: Option<Vec<TranscriptionWord>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionDiarized {
    pub duration: f64,
    pub segments: Vec<TranscriptionDiarizedSegment>,
    pub task: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AudioUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    pub text: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranslationVerbose {
    pub duration: f64,
    pub language: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<TranscriptionSegment>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudioUsage {
    Tokens(AudioTokenUsage),
    Duration(AudioDurationUsage),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<AudioTokenUsageType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_details: Option<AudioInputTokenDetails>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioTokenUsageType {
    #[serde(rename = "tokens")]
    Tokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDurationUsage {
    pub seconds: f64,
    #[serde(rename = "type")]
    pub type_: AudioDurationUsageType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDurationUsageType {
    #[serde(rename = "duration")]
    Duration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioInputTokenDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionLanguage {
    pub code: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionLogprob {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprob: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub id: u64,
    pub avg_logprob: f64,
    pub compression_ratio: f64,
    pub end: f64,
    pub no_speech_prob: f64,
    pub seek: u64,
    pub start: f64,
    pub temperature: f64,
    pub text: String,
    pub tokens: Vec<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionWord {
    pub end: f64,
    pub start: f64,
    pub word: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptionDiarizedSegment {
    pub id: String,
    pub end: f64,
    pub speaker: String,
    pub start: f64,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: TranscriptionSegmentType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranscriptionSegmentType {
    #[serde(rename = "transcript.text.segment")]
    TranscriptTextSegment,
}
