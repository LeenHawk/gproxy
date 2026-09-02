use serde::{Deserialize, Serialize};

use super::super::{EmbedContentRequest, EmbedContentResponse, JsonMap, Status};
use super::{BatchOperation, BatchResourceFields};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AsyncBatchEmbedContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<EmbedContentBatch>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

pub type AsyncBatchEmbedContentResponse = BatchOperation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmbedContentBatchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentBatch {
    #[serde(flatten)]
    pub resource: BatchResourceFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_config: Option<EmbedContentInputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<EmbedContentBatchOutput>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentInputConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<EmbedContentInputSource>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum EmbedContentInputSource {
    File {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    Requests {
        requests: InlinedEmbedRequests,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedRequests {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<InlinedEmbedRequest>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InlinedEmbedRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<EmbedContentRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentBatchOutput {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<EmbedContentBatchOutputData>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum EmbedContentBatchOutputData {
    ResponsesFile {
        #[serde(rename = "responsesFile")]
        responses_file: String,
    },
    InlinedResponses {
        #[serde(rename = "inlinedResponses")]
        inlined_responses: InlinedEmbedResponses,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedResponses {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlined_responses: Vec<InlinedEmbedResponse>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InlinedEmbedResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<InlinedEmbedResponseOutput>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InlinedEmbedResponseOutput {
    Error { error: Status },
    Response { response: EmbedContentResponse },
}

pub type AsyncBatchEmbedContentRequestBody = AsyncBatchEmbedContentRequest;
pub type AsyncBatchEmbedContentResponseBody = AsyncBatchEmbedContentResponse;
pub type UpdateEmbedContentBatchRequestBody = EmbedContentBatch;
pub type UpdateEmbedContentBatchResponseBody = EmbedContentBatch;
pub type InputEmbedContentConfig = EmbedContentInputConfig;
pub type InputEmbedContentConfigSource = EmbedContentInputSource;
pub type InlinedEmbedContentRequests = InlinedEmbedRequests;
pub type InlinedEmbedContentRequest = InlinedEmbedRequest;
pub type InlinedEmbedContentResponses = InlinedEmbedResponses;
pub type InlinedEmbedContentResponse = InlinedEmbedResponse;
pub type InlinedEmbedContentResponseOutput = InlinedEmbedResponseOutput;
