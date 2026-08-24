use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{OpenAiModelId, ResponseToolChoice, Rest};
use crate::openai::generate_content::responses::ResponseTool;

use super::audio::RealtimeAudio;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeSession {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<RealtimeSessionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Vec<RealtimeOutputModality>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<RealtimeAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ResponseTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ResponseToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<RealtimeMaxTokens>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

pub type RealtimeSessionConfig = RealtimeSession;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeMaxTokens {
    Count(u64),
    Infinite(String),
    Raw(Value),
}

extensible_string!(RealtimeSessionType, KnownRealtimeSessionType {
    Realtime => "realtime", Transcription => "transcription",
});
extensible_string!(RealtimeOutputModality, KnownRealtimeOutputModality {
    Text => "text", Audio => "audio",
});
