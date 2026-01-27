use serde::{Deserialize, Serialize};

use crate::openai::create_chat_completions::types::{
    ChatCompletionChoiceLogprobs, ChatCompletionFinishReason, ChatCompletionStreamResponseDelta,
    CompletionUsage, ServiceTier,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChatCompletionChunkObjectType {
    #[serde(rename = "chat.completion.chunk")]
    ChatCompletionChunk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionStreamChoice {
    pub index: i64,
    pub delta: ChatCompletionStreamResponseDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<ChatCompletionChoiceLogprobs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<ChatCompletionFinishReason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateChatCompletionStreamResponse {
    pub id: String,
    pub object: ChatCompletionChunkObjectType,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChatCompletionStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<CompletionUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_fingerprint: Option<String>,
}
