use serde::{Deserialize, Serialize};

use super::super::{DocumentBlock, JsonObject, StopReason, ToolReferenceBlock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebFetchResultBlock {
    pub content: DocumentBlock,
    #[serde(rename = "type")]
    pub type_: WebFetchResultBlockType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved_at: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebFetchResultBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisorResultBlock {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: AdvisorResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvisorResultBlockType {
    #[serde(rename = "advisor_result")]
    AdvisorResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdvisorRedactedResultBlock {
    pub encrypted_content: String,
    #[serde(rename = "type")]
    pub type_: AdvisorRedactedResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdvisorRedactedResultBlockType {
    #[serde(rename = "advisor_redacted_result")]
    AdvisorRedactedResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: CodeExecutionOutputBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeExecutionOutputBlockType {
    #[serde(rename = "code_execution_output")]
    CodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeExecutionResultBlock {
    pub content: Vec<CodeExecutionOutputBlock>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_: CodeExecutionResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeExecutionResultBlockType {
    #[serde(rename = "code_execution_result")]
    CodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncryptedCodeExecutionResultBlock {
    pub content: Vec<CodeExecutionOutputBlock>,
    pub encrypted_stdout: String,
    pub return_code: i64,
    pub stderr: String,
    #[serde(rename = "type")]
    pub type_: EncryptedCodeExecutionResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EncryptedCodeExecutionResultBlockType {
    #[serde(rename = "encrypted_code_execution_result")]
    EncryptedCodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BashCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionOutputBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BashCodeExecutionOutputBlockType {
    #[serde(rename = "bash_code_execution_output")]
    BashCodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BashCodeExecutionResultBlock {
    pub content: Vec<BashCodeExecutionOutputBlock>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BashCodeExecutionResultBlockType {
    #[serde(rename = "bash_code_execution_result")]
    BashCodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSearchToolSearchResultBlock {
    pub tool_references: Vec<ToolReferenceBlock>,
    #[serde(rename = "type")]
    pub type_: ToolSearchToolSearchResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSearchToolSearchResultBlockType {
    #[serde(rename = "tool_search_tool_search_result")]
    ToolSearchToolSearchResult,
}
