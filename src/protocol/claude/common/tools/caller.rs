use serde::{Deserialize, Serialize};

use super::super::{JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Caller {
    Direct(DirectCaller),
    ServerTool(ServerToolCaller),
    ServerTool20260120(ServerToolCaller20260120),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectCaller {
    #[serde(rename = "type")]
    pub type_: DirectCallerType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DirectCallerType {
    #[serde(rename = "direct")]
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolCaller {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: ServerToolCallerType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerToolCallerType {
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolCaller20260120 {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: ServerToolCaller20260120Type,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerToolCaller20260120Type {
    #[serde(rename = "code_execution_20260120")]
    CodeExecution20260120,
}
