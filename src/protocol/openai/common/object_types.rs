use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatCompletionObjectType {
    #[serde(rename = "chat.completion")]
    #[default]
    ChatCompletion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatCompletionChunkObjectType {
    #[serde(rename = "chat.completion.chunk")]
    ChatCompletionChunk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseObjectType {
    #[serde(rename = "response")]
    Response,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCompactionObjectType {
    #[serde(rename = "response.compaction")]
    ResponseCompaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputTokensObjectType {
    #[serde(rename = "response.input_tokens")]
    ResponseInputTokens,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListObjectType {
    #[serde(rename = "list")]
    List,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelObjectType {
    #[serde(rename = "model")]
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingObjectType {
    #[serde(rename = "embedding")]
    Embedding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationObjectType {
    #[serde(rename = "conversation")]
    Conversation,
}
