use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{EmbeddingObjectType, ListObjectType, OpenAiModelId, Rest};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateEmbeddingRequest {
    pub input: EmbeddingInput,
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding_format: Option<EmbeddingEncodingFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Text(String),
    TextList(Vec<String>),
    TokenList(Vec<i64>),
    TokenLists(Vec<Vec<i64>>),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingEncodingFormat {
    Known(KnownEmbeddingEncodingFormat),
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownEmbeddingEncodingFormat {
    Float,
    Base64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateEmbeddingResponse {
    pub data: Vec<Embedding>,
    pub model: OpenAiModelId,
    pub object: ListObjectType,
    pub usage: EmbeddingUsage,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    pub embedding: EmbeddingVector,
    pub index: u32,
    pub object: EmbeddingObjectType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingVector {
    Float(Vec<f64>),
    Base64(String),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingUsage {
    pub prompt_tokens: u64,
    pub total_tokens: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedding_round_trip_preserves_unknown_fields_and_variants() {
        let value = json!({
            "input": {"future_input": true},
            "model": "text-embedding-future",
            "encoding_format": "packed",
            "future_request": {"enabled": true}
        });
        let request: CreateEmbeddingRequest = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(request).unwrap(), value);

        let response = json!({
            "data": [{
                "embedding": "AAEC",
                "index": 0,
                "object": "embedding",
                "future_embedding": 7
            }],
            "model": "text-embedding-future",
            "object": "list",
            "usage": {"prompt_tokens": 1, "total_tokens": 1, "future_usage": 2},
            "future_response": "kept"
        });
        let parsed: CreateEmbeddingResponse = serde_json::from_value(response.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), response);
    }
}
