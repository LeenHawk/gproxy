use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{CacheControl, JsonObject, McpServerType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolset {
    pub mcp_server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolsetType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub configs: BTreeMap<String, McpToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<McpToolConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpToolsetType {
    #[serde(rename = "mcp_toolset")]
    McpToolset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: McpServerType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<McpToolConfiguration>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
