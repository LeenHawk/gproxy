use serde::{Deserialize, Serialize};

use super::super::{CacheControl, ClaudeModel, JsonObject, TypedObject};
use super::TextBlock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContainerUploadBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: ContainerUploadBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContainerUploadBlockType {
    #[serde(rename = "container_upload")]
    ContainerUpload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(rename = "type")]
    pub type_: CompactionBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CompactionBlockType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MidConversationSystemBlock {
    pub content: Vec<MidConversationSystemContentBlock>,
    #[serde(rename = "type")]
    pub type_: MidConversationSystemBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MidConversationSystemContentBlock {
    Text(TextBlock),
    ToolAddition(ToolAdditionBlock),
    ToolRemoval(ToolRemovalBlock),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolAdditionBlock {
    pub tool: ToolChangeReference,
    #[serde(rename = "type")]
    pub type_: ToolAdditionBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolRemovalBlock {
    pub tool: ToolChangeReference,
    #[serde(rename = "type")]
    pub type_: ToolRemovalBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChangeReference {
    McpTool(McpToolChangeReference),
    McpToolset(McpToolsetChangeReference),
    Tool(ToolChangeToolReference),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChangeToolReference {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ToolChangeToolReferenceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolChangeReference {
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolChangeReferenceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolsetChangeReference {
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolsetChangeReferenceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolAdditionBlockType {
    #[serde(rename = "tool_addition")]
    ToolAddition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolRemovalBlockType {
    #[serde(rename = "tool_removal")]
    ToolRemoval,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolChangeToolReferenceType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpToolChangeReferenceType {
    #[serde(rename = "mcp_tool_reference")]
    McpToolReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpToolsetChangeReferenceType {
    #[serde(rename = "mcp_toolset_reference")]
    McpToolsetReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MidConversationSystemBlockType {
    #[serde(rename = "mid_conv_system")]
    MidConversationSystem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackBlock {
    pub from: FallbackInfo,
    pub to: FallbackInfo,
    #[serde(rename = "type")]
    pub type_: FallbackBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackInfo {
    pub model: ClaudeModel,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FallbackBlockType {
    #[serde(rename = "fallback")]
    Fallback,
}
