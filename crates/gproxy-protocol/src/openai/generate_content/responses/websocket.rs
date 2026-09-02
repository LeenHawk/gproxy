use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{Metadata, Rest};

use super::{ResponseCreateRequest, ResponseItem};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseWebSocketRequest {
    ResponseCreate(Box<ResponseCreateWebSocketRequest>),
    ResponseInject(ResponseInjectWebSocketRequest),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCreateWebSocketRequest {
    #[serde(rename = "type")]
    pub type_: ResponseCreateWebSocketRequestType,
    #[serde(flatten)]
    pub response: ResponseCreateRequest,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_metadata: Option<Metadata>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseCreateWebSocketRequestType {
    #[serde(rename = "response.create")]
    ResponseCreate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInjectWebSocketRequest {
    #[serde(rename = "type")]
    pub type_: ResponseInjectWebSocketRequestType,
    pub response_id: String,
    pub input: Vec<ResponseItem>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ResponseInjectWebSocketRequestType {
    #[serde(rename = "response.inject")]
    ResponseInject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInjectCreatedEvent {
    pub response_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInjectFailedEvent {
    pub response_id: String,
    pub input: Vec<ResponseItem>,
    pub error: ResponseInjectError,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence_number: Option<u64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInjectError {
    pub code: String,
    pub message: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
