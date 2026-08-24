use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Status;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VeoPredictLongRunningRequest {
    // models.predictLongRunning defines each instance as google.protobuf.Value.
    pub instances: Vec<Value>,
    // models.predictLongRunning defines its parameters as google.protobuf.Value.
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
    // Long-running operation metadata is a service-specific protobuf Any object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Status>,
    // A successful long-running result is a service-specific protobuf Any object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub rest: serde_json::Map<String, Value>,
}
