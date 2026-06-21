use serde::{Deserialize, Serialize};

use super::super::server_tools::ResponseWebSearchToolResultContent;
use super::super::{Caller, Citation, JsonObject, ServerToolUseName, StringOrArray};
use super::{
    CompactionBlockType, ContainerUploadBlockType, ServerToolUseBlockType, TextBlockType,
    ToolUseBlockType, WebSearchResultBlockType, WebSearchToolResultBlockType,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseTextBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Citation>>,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: TextBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseServerToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: ServerToolUseName,
    #[serde(rename = "type")]
    pub type_: ServerToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebSearchToolResultBlock {
    pub content: ResponseWebSearchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: WebSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebSearchResultBlock {
    pub encrypted_content: String,
    pub page_age: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: WebSearchResultBlockType,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolReferenceBlock {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub type_: ResponseToolReferenceBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpToolUseBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    McpToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpToolResultBlock {
    pub content: ResponseMcpToolResultContent,
    pub is_error: bool,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpToolResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    McpToolResult,
}

pub type ResponseMcpToolResultContent = StringOrArray<ResponseTextBlock>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseContainerUploadBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: ContainerUploadBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCompactionBlock {
    pub content: Option<String>,
    pub encrypted_content: String,
    #[serde(rename = "type")]
    pub type_: CompactionBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}
