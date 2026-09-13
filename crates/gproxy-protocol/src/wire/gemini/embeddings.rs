//! Gemini embedContent and batchEmbedContents wire shapes. Source: Embeddings.md.
use super::content::Content;
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EmbedContentRequestBody {
    pub content: Content,
    #[serde(alias = "task_type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<GeminiTaskType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(alias = "output_dimensionality")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<i64>,
    #[serde(alias = "embed_content_config")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_content_config: Option<EmbedContentConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type EmbedContentRequest = WireRequest<EmbedContentRequestBody>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EmbedContentResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<ContentEmbedding>,
    #[serde(alias = "usage_metadata")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<EmbeddingUsageMetadata>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type EmbedContentResponse = WireResponse<EmbedContentResponseBody>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EmbedContentConfig {
    #[serde(alias = "document_ocr", skip_serializing_if = "Option::is_none")]
    pub document_ocr: Option<bool>,
    #[serde(
        alias = "audio_track_extraction",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio_track_extraction: Option<bool>,
    #[serde(alias = "task_type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<GeminiTaskType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(alias = "output_dimensionality")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<i64>,
    #[serde(alias = "auto_truncate")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_truncate: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum GeminiTaskType {
    #[serde(rename = "TASK_TYPE_UNSPECIFIED")]
    Unspecified,
    #[serde(rename = "RETRIEVAL_QUERY")]
    RetrievalQuery,
    #[serde(rename = "RETRIEVAL_DOCUMENT")]
    RetrievalDocument,
    #[serde(rename = "SEMANTIC_SIMILARITY")]
    SemanticSimilarity,
    #[serde(rename = "CLASSIFICATION")]
    Classification,
    #[serde(rename = "CLUSTERING")]
    Clustering,
    #[serde(rename = "QUESTION_ANSWERING")]
    QuestionAnswering,
    #[serde(rename = "FACT_VERIFICATION")]
    FactVerification,
    #[serde(rename = "CODE_RETRIEVAL_QUERY")]
    CodeRetrievalQuery,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ContentEmbedding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<serde_json::Number>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<Vec<i64>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct EmbeddingUsageMetadata {
    #[serde(alias = "prompt_token_count", skip_serializing_if = "Option::is_none")]
    pub prompt_token_count: Option<i64>,
    #[serde(
        alias = "prompt_token_details",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_token_details: Option<Vec<super::count_tokens::ModalityTokenCount>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchEmbedContentsRequestBody {
    pub requests: Vec<BatchEmbedContentRequest>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type BatchEmbedContentsRequest = WireRequest<BatchEmbedContentsRequestBody>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchEmbedContentRequest {
    pub model: String,
    pub content: Content,
    #[serde(alias = "task_type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<GeminiTaskType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(alias = "output_dimensionality")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dimensionality: Option<i64>,
    #[serde(alias = "embed_content_config")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed_content_config: Option<EmbedContentConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchEmbedContentsResponseBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeddings: Option<Vec<ContentEmbedding>>,
    #[serde(alias = "usage_metadata")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<EmbeddingUsageMetadata>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type BatchEmbedContentsResponse = WireResponse<BatchEmbedContentsResponseBody>;
