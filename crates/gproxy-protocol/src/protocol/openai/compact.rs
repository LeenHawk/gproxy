use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::*;
use super::generate_content::{
    ComputerScreenshot, ResponseInput, ResponseInputContentPart, ResponseMessageItemType,
    ResponseOutputContentPart, ResponseUsage, TypedResponseItem, UnknownResponseItem,
};

pub type CompactResponseWireModel =
    OpenAiWireModel<CompactResponseRequestBody, CompactedResponseObject>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactResponseRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponseInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub model: OpenAiModelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<CompactServiceTier>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactedResponseObject {
    pub id: String,
    pub created_at: u64,
    pub object: ResponseCompactionObjectType,
    pub output: Vec<CompactResponseItem>,
    pub usage: ResponseUsage,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompactResponseItem {
    Message(CompactMessageItem),
    Typed(TypedResponseItem),
    Unknown(UnknownResponseItem),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactMessageItem {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: ResponseMessageItemType,
    pub content: Vec<CompactMessageContentPart>,
    pub role: CompactMessageRole,
    pub status: ResponseItemLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompactMessageContentPart {
    Input(ResponseInputContentPart),
    Output(ResponseOutputContentPart),
    Text(CompactTextContent),
    SummaryText(CompactSummaryTextContent),
    ComputerScreenshot(ComputerScreenshot),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: CompactTextContentType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactTextContentType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactSummaryTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: CompactSummaryTextContentType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactSummaryTextContentType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactMessageRole {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "critic")]
    Critic,
    #[serde(rename = "discriminator")]
    Discriminator,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactServiceTier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "flex")]
    Flex,
    #[serde(rename = "priority")]
    Priority,
}
