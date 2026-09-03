use serde::{Deserialize, Serialize};

use super::super::{BatchState, JsonMap, Status};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GetBatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ListBatchesResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<BatchOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_page_token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unreachable: Vec<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

pub type CancelBatchRequest = GetBatchRequest;
pub type DeleteBatchRequest = GetBatchRequest;
pub type CancelBatchResponse = JsonMap;
pub type DeleteBatchResponse = JsonMap;
pub type CancelBatchResponseBody = CancelBatchResponse;
pub type DeleteBatchResponseBody = DeleteBatchResponse;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<JsonMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub result: Option<BatchOperationResult>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

pub type Operation = BatchOperation;
pub type OperationResult = BatchOperationResult;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum BatchOperationResult {
    Error { error: Status },
    Response { response: JsonMap },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub successful_request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_request_count: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_request_count: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonMap::is_empty")]
    pub rest: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BatchResourceFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
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
}
