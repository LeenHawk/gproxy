use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::*;
use crate::openai::generate_content::responses::{
    ComputerScreenshot, ResponseInput, ResponseInputContentPart, ResponseMessageItemType,
    ResponseOutputContentPart, ResponseUsage, TypedResponseItem,
};

pub type CompactResponseWireModel =
    OpenAiWireModel<CompactResponseRequestBody, CompactedResponseObject>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactResponseRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<ResponseInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_options: Option<PromptCacheOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<CompactServiceTier>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactedResponseObject {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    pub object: ResponseCompactionObjectType,
    pub output: Vec<CompactResponseItem>,
    pub usage: ResponseUsage,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompactResponseItem {
    Message(CompactMessageItem),
    Typed(Box<TypedResponseItem>),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactMessageItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub type_: ResponseMessageItemType,
    pub content: Vec<CompactMessageContentPart>,
    pub role: CompactMessageRole,
    pub status: ResponseItemLifecycleStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponsePhase>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompactMessageContentPart {
    Input(ResponseInputContentPart),
    Output(ResponseOutputContentPart),
    Text(CompactTextContent),
    SummaryText(CompactSummaryTextContent),
    ComputerScreenshot(ComputerScreenshot),
    Unknown(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: CompactTextContentType,
    #[serde(default, flatten)]
    pub rest: Rest,
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
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompactSummaryTextContentType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactMessageRole {
    Unknown,
    User,
    Assistant,
    System,
    Critic,
    Discriminator,
    Developer,
    Tool,
}

pub type CompactServiceTier = ServiceTier;
