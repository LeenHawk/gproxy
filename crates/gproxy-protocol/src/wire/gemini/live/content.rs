//! Live content extends the current shared Part with Developer API audio
//! transcription and media processing fields, confirmed by _Part_to_mldev.
use super::realtime::LiveBlob;
use super::server::BidiGenerateContentTranscription;
use crate::Rest;
use crate::gemini::content::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LiveContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<LivePart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct LivePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thought_signature")]
    pub thought_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "part_metadata")]
    pub part_metadata: Option<Rest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "media_resolution")]
    pub media_resolution: Option<MediaResolution>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "inline_data")]
    pub inline_data: Option<LiveBlob>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "function_call")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "function_response")]
    pub function_response: Option<FunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "file_data")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "executable_code")]
    pub executable_code: Option<ExecutableCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "code_execution_result")]
    pub code_execution_result: Option<CodeExecutionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_call")]
    pub tool_call: Option<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "tool_response")]
    pub tool_response: Option<ToolResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "video_metadata")]
    pub video_metadata: Option<VideoMetadata>,
    #[serde(
        alias = "audio_transcription",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_transcription: Option<BidiGenerateContentTranscription>,
    #[serde(
        alias = "media_processing",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub media_processing: Option<LiveMediaProcessing>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum LiveMediaProcessing {
    #[serde(rename = "MEDIA_PROCESSING_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "STATIC")]
    Static,
    #[serde(rename = "AGENTIC")]
    Agentic,
}
