use serde::{Deserialize, Serialize};

macro_rules! marker {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChatCompletionObjectType {
    #[serde(rename = "chat.completion")]
    #[default]
    ChatCompletion,
}
marker!(
    ChatCompletionChunkObjectType,
    ChatCompletionChunk,
    "chat.completion.chunk"
);
marker!(ResponseObjectType, Response, "response");
marker!(
    ResponseCompactionObjectType,
    ResponseCompaction,
    "response.compaction"
);
marker!(
    ResponseInputTokensObjectType,
    ResponseInputTokens,
    "response.input_tokens"
);
marker!(ListObjectType, List, "list");
marker!(ModelObjectType, Model, "model");
marker!(EmbeddingObjectType, Embedding, "embedding");
marker!(ConversationObjectType, Conversation, "conversation");
