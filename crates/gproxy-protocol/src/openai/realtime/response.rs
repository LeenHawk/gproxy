use serde::{Deserialize, Serialize};

use crate::openai::common::{Metadata, ResponseToolChoice, Rest};
use crate::openai::generate_content::responses::ResponseTool;

use super::audio::RealtimeAudio;
use super::items::RealtimeItem;
use super::session::{RealtimeMaxTokens, RealtimeOutputModality};

extensible_string!(RealtimeResponseStatus, KnownRealtimeResponseStatus {
    InProgress => "in_progress", Completed => "completed", Cancelled => "cancelled",
    Incomplete => "incomplete", Failed => "failed",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<RealtimeResponseStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_details: Option<RealtimeStatusDetails>,
    pub output: Vec<RealtimeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<RealtimeOutputModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<RealtimeUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeStatusDetails {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RealtimeError>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RealtimeResponseOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Vec<RealtimeItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<RealtimeOutputModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<RealtimeAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponseToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<RealtimeMaxTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_token_details: Option<RealtimeInputTokenDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_token_details: Option<RealtimeOutputTokenDetails>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeInputTokenDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeOutputTokenDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeError {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub param: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeRateLimit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_seconds: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
