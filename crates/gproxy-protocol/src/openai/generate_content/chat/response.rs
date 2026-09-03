use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::{ChatAnnotation, ChatAudio, ChatChoiceLogprobs, ChatToolCall, CompletionUsage};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    pub model: OpenAiModelId,
    #[serde(default)]
    pub object: ChatCompletionObjectType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ChatCompletionModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompletionUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatCompletionChoice {
    pub finish_reason: ChatFinishReason,
    pub index: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<ChatChoiceLogprobs>,
    pub message: ChatMessage,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatCompletionModeration {
    pub input: ChatCompletionModerationOutcome,
    pub output: ChatCompletionModerationOutcome,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatCompletionModerationOutcome {
    Results(ChatCompletionModerationResults),
    Error(ModerationError),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatCompletionModerationResults {
    pub model: OpenAiModelId,
    pub results: Vec<ModerationResult>,
    #[serde(rename = "type")]
    pub type_: ChatCompletionModerationResultsType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatCompletionModerationResultsType {
    #[serde(rename = "moderation_results")]
    ModerationResults,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ChatMessage {
    pub role: ChatCompletionMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<ChatAnnotation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<ChatAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_call: Option<FunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ChatToolCall>>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatCompletionMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}
