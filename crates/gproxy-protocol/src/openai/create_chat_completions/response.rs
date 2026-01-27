use serde::{Deserialize, Serialize};

use crate::openai::create_chat_completions::types::{
    ChatCompletionChoiceLogprobs, ChatCompletionFinishReason, ChatCompletionResponseMessage,
    CompletionUsage, ServiceTier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatCompletionObjectType {
    #[serde(rename = "chat.completion")]
    ChatCompletion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionChoice {
    pub index: i64,
    pub message: ChatCompletionResponseMessage,
    pub finish_reason: ChatCompletionFinishReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<ChatCompletionChoiceLogprobs>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateChatCompletionResponse {
    pub id: String,
    pub object: ChatCompletionObjectType,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompletionUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
}
