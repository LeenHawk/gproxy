use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;

use super::{ResponseEasyInputContent, ResponseInputContentPart, ResponseMessageOutputContentPart};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseMessageItem {
    Output(ResponseOutputMessageItem),
    Input(ResponseInputMessageItem),
    EasyInput(ResponseEasyInputMessageItem),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputMessageItem {
    #[serde(rename = "type")]
    pub type_: ResponseMessageItemType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: ResponseOutputMessageRole,
    pub content: Vec<ResponseMessageOutputContentPart>,
    pub status: ResponseItemLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputMessageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseMessageItemType>,
    pub role: ResponseInputMessageRole,
    pub content: Vec<ResponseInputContentPart>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemLifecycleStatus>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseEasyInputMessageItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseMessageItemType>,
    pub role: ResponseEasyInputMessageRole,
    pub content: ResponseEasyInputContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMessageItemType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseInputMessageRole {
    User,
    System,
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseEasyInputMessageRole {
    User,
    Assistant,
    System,
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAgent {
    pub agent_name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseCaller {
    Direct(ResponseDirectCaller),
    Program(ResponseProgramCaller),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseDirectCaller {
    #[serde(rename = "type")]
    pub type_: ResponseDirectCallerType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseDirectCallerType {
    #[serde(rename = "direct")]
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseProgramCaller {
    #[serde(rename = "type")]
    pub type_: ResponseProgramCallerType,
    pub caller_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseProgramCallerType {
    #[serde(rename = "program")]
    Program,
}
