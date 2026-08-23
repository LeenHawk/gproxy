use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::{Metadata, Rest};

use super::ResponseCreateRequest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseWebSocketRequest {
    ResponseCreate(Box<ResponseCreateWebSocketRequest>),
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
pub enum ResponseCreateWebSocketRequestType {
    #[serde(rename = "response.create")]
    ResponseCreate,
}
