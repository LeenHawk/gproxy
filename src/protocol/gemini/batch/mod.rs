use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{BatchState, ExtraFields, JsonMap, Status};
use super::embeddings::{EmbedContentRequest, EmbedContentResponse};
use super::generate_content::{GenerateContentRequest, GenerateContentResponse};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatchGenerateContentRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<GenerateContentBatch>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type BatchGenerateContentResponseBody = Operation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AsyncBatchEmbedContentRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<EmbedContentBatch>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type AsyncBatchEmbedContentResponseBody = Operation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GetBatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchesRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_partial_success: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListBatchesResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreachable: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CancelBatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type CancelBatchResponseBody = JsonMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteBatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type DeleteBatchResponseBody = JsonMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGenerateContentBatchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type UpdateGenerateContentBatchRequestBody = GenerateContentBatch;
pub type UpdateGenerateContentBatchResponseBody = GenerateContentBatch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEmbedContentBatchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

pub type UpdateEmbedContentBatchRequestBody = EmbedContentBatch;
pub type UpdateEmbedContentBatchResponseBody = EmbedContentBatch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub result: Option<OperationResult>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OperationResult {
    Error { error: Status },
    Response { response: JsonMap },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentBatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_config: Option<InputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<GenerateContentBatchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_stats: Option<BatchStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<BatchState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InputConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<InputConfigSource>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputConfigSource {
    String {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    Requests {
        requests: InlinedRequests,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedRequests {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<InlinedRequest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<GenerateContentRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerateContentBatchOutput {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<GenerateContentBatchOutputData>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GenerateContentBatchOutputData {
    ResponsesFile {
        #[serde(rename = "responsesFile")]
        responses_file: String,
    },
    InlinedResponses {
        #[serde(rename = "inlinedResponses")]
        inlined_responses: InlinedResponses,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedResponses {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlined_responses: Vec<InlinedResponse>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<InlinedResponseOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InlinedResponseOutput {
    Error {
        error: Status,
    },
    Response {
        response: Box<GenerateContentResponse>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BatchStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful_request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_request_count: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentBatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_config: Option<InputEmbedContentConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<EmbedContentBatchOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_stats: Option<BatchStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<BatchState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InputEmbedContentConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<InputEmbedContentConfigSource>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputEmbedContentConfigSource {
    String {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    Requests {
        requests: InlinedEmbedContentRequests,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedContentRequests {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<InlinedEmbedContentRequest>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<EmbedContentRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EmbedContentBatchOutput {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<EmbedContentBatchOutputData>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EmbedContentBatchOutputData {
    ResponsesFile {
        #[serde(rename = "responsesFile")]
        responses_file: String,
    },
    InlinedResponses {
        #[serde(rename = "inlinedResponses")]
        inlined_responses: InlinedEmbedContentResponses,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedContentResponses {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlined_responses: Vec<InlinedEmbedContentResponse>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InlinedEmbedContentResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<InlinedEmbedContentResponseOutput>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InlinedEmbedContentResponseOutput {
    Error { error: Status },
    Response { response: EmbedContentResponse },
}
