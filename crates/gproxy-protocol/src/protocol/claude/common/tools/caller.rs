use serde::{Deserialize, Serialize};

use super::super::{JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum Caller {
    Direct(DirectCaller),
    ServerTool(ServerToolCaller),
    ServerTool20260120(ServerToolCaller20260120),
    ServerTool20260521(ServerToolCaller20260521),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DirectCaller {
    #[serde(rename = "type")]
    pub type_: DirectCallerType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum DirectCallerType {
    #[serde(rename = "direct")]
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ServerToolCaller {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: ServerToolCallerType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ServerToolCallerType {
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ServerToolCaller20260120 {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: ServerToolCaller20260120Type,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ServerToolCaller20260120Type {
    #[serde(rename = "code_execution_20260120")]
    CodeExecution20260120,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ServerToolCaller20260521 {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: ServerToolCaller20260521Type,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ServerToolCaller20260521Type {
    #[serde(rename = "code_execution_20260521")]
    CodeExecution20260521,
}
