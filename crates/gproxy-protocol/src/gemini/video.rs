use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Status;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VeoPredictLongRunningRequest {
    // models.predictLongRunning defines each instance as google.protobuf.Value.
    pub instances: Vec<Value>,
    /// `upstream_docs/gemini/docs/Models.md`, `models.predictLongRunning.parameters`:
    /// prediction parameters are an arbitrary `google.protobuf.Value`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VeoOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `upstream_docs/gemini/docs/Batch API.md`, `Resource: Operation.metadata`:
    /// service-specific `google.protobuf.Any` JSON, including its `@type` URI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Status>,
    /// `upstream_docs/gemini/docs/Batch API.md`, `Resource: Operation.response`:
    /// the service-specific result as `google.protobuf.Any` JSON with `@type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, Value>,
}
