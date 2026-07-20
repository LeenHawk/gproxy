use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::super::common::*;
use super::{ResponseEasyInputContent, ResponseInputContentPart, ResponseMessageOutputContentPart};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ResponseMessageItem {
    Output(ResponseOutputMessageItem),
    Input(ResponseInputMessageItem),
    EasyInput(ResponseEasyInputMessageItem),
}

impl<'de> Deserialize<'de> for ResponseMessageItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let role = value.get("role").and_then(Value::as_str);
        let has_id = value.get("id").is_some();
        let has_status = value.get("status").is_some();

        if role == Some("assistant") && has_id && has_status {
            return serde_json::from_value(value)
                .map(Self::Output)
                .map_err(de::Error::custom);
        }

        if has_id || has_status {
            return serde_json::from_value(value)
                .map(Self::Input)
                .map_err(de::Error::custom);
        }

        serde_json::from_value(value)
            .map(Self::EasyInput)
            .map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputMessageItem {
    #[serde(rename = "type")]
    pub type_: ResponseMessageItemType,
    pub id: String,
    pub role: ResponseOutputMessageRole,
    pub content: Vec<ResponseMessageOutputContentPart>,
    pub status: ResponseItemLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseEasyInputMessageItem {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseMessageItemType>,
    pub role: ResponseEasyInputMessageRole,
    pub content: ResponseEasyInputContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
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
pub enum ResponseInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseEasyInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAgent {
    pub agent_name: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseCaller {
    #[serde(rename = "direct")]
    Direct {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "program")]
    Program {
        caller_id: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}
