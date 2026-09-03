use serde::{Deserialize, Serialize};

use super::super::{GenerateContentRequest, GenerateContentResponse, JsonMap, Status};
use super::{BatchOperation, BatchResourceFields};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchGenerateContentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<GenerateContentBatch>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

pub type BatchGenerateContentResponse = BatchOperation;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct UpdateGenerateContentBatchQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_mask: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GenerateContentBatch {
    #[serde(flatten)]
    pub resource: BatchResourceFields,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_config: Option<GenerateContentInputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<GenerateContentBatchOutput>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GenerateContentInputConfig {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub source: Option<GenerateContentInputSource>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum GenerateContentInputSource {
    File {
        #[serde(rename = "fileName")]
        file_name: String,
    },
    Requests {
        requests: InlinedGenerateRequests,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InlinedGenerateRequests {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requests: Vec<InlinedGenerateRequest>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Default, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InlinedGenerateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<GenerateContentRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GenerateContentBatchOutput {
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<GenerateContentBatchOutputData>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum GenerateContentBatchOutputData {
    ResponsesFile {
        #[serde(rename = "responsesFile")]
        responses_file: String,
    },
    InlinedResponses {
        #[serde(rename = "inlinedResponses")]
        inlined_responses: InlinedGenerateResponses,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InlinedGenerateResponses {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inlined_responses: Vec<InlinedGenerateResponse>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, Default, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct InlinedGenerateResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub output: Option<InlinedGenerateResponseOutput>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum InlinedGenerateResponseOutput {
    Error {
        error: Status,
    },
    Response {
        response: Box<GenerateContentResponse>,
    },
}

pub type BatchGenerateContentRequestBody = BatchGenerateContentRequest;
pub type BatchGenerateContentResponseBody = BatchGenerateContentResponse;
pub type UpdateGenerateContentBatchRequestBody = GenerateContentBatch;
pub type UpdateGenerateContentBatchResponseBody = GenerateContentBatch;
pub type InputConfig = GenerateContentInputConfig;
pub type InputConfigSource = GenerateContentInputSource;
pub type InlinedRequests = InlinedGenerateRequests;
pub type InlinedRequest = InlinedGenerateRequest;
pub type InlinedResponses = InlinedGenerateResponses;
pub type InlinedResponse = InlinedGenerateResponse;
pub type InlinedResponseOutput = InlinedGenerateResponseOutput;
