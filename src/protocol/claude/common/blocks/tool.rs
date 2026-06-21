use serde::{Deserialize, Serialize};

use super::super::{
    CacheControl, Caller, JsonObject, ServerToolUseName, WebSearchToolResultContent,
};
use super::content::{McpToolResultContent, ToolResultContent};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolUseBlockType {
    #[serde(rename = "tool_use")]
    ToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: ToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolResultBlockType {
    #[serde(rename = "tool_result")]
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolReferenceBlock {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub type_: ToolReferenceBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: ServerToolUseName,
    #[serde(rename = "type")]
    pub type_: ServerToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerToolUseBlockType {
    #[serde(rename = "server_tool_use")]
    ServerToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchToolResultBlock {
    pub content: WebSearchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: WebSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchResultBlock {
    pub encrypted_content: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: WebSearchResultBlockType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum McpToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    McpToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolResultBlock {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: McpToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<McpToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum McpToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    McpToolResult,
}
