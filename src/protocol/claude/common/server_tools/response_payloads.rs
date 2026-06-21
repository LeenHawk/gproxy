use serde::{Deserialize, Serialize};

use super::super::{
    Base64PdfSource, DocumentBlockType, JsonObject, PlainTextSource, ResponseToolReferenceBlock,
    StopReason,
};
use super::{
    TextEditorCodeExecutionFileType, TextEditorCodeExecutionStrReplaceResultBlockType,
    TextEditorCodeExecutionToolResultErrorCode, TextEditorCodeExecutionToolResultErrorType,
    TextEditorCodeExecutionViewResultBlockType, ToolSearchToolResultErrorCode,
    ToolSearchToolResultErrorType,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebFetchResultBlock {
    pub content: ResponseWebFetchDocumentBlock,
    pub retrieved_at: String,
    #[serde(rename = "type")]
    pub type_: ResponseWebFetchResultBlockType,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseWebFetchResultBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebFetchDocumentBlock {
    pub citations: ResponseWebFetchCitationConfig,
    pub source: ResponseWebFetchDocumentSource,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: DocumentBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebFetchCitationConfig {
    pub enabled: bool,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseWebFetchDocumentSource {
    Base64(Base64PdfSource),
    Text(PlainTextSource),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAdvisorResultBlock {
    pub stop_reason: StopReason,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseAdvisorResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseAdvisorResultBlockType {
    #[serde(rename = "advisor_result")]
    AdvisorResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseAdvisorRedactedResultBlock {
    pub encrypted_content: String,
    pub stop_reason: StopReason,
    #[serde(rename = "type")]
    pub type_: ResponseAdvisorRedactedResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseAdvisorRedactedResultBlockType {
    #[serde(rename = "advisor_redacted_result")]
    AdvisorRedactedResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseTextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: TextEditorCodeExecutionFileType,
    pub num_lines: u64,
    pub start_line: u64,
    pub total_lines: u64,
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionViewResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseTextEditorCodeExecutionStrReplaceResultBlock {
    pub lines: Vec<String>,
    pub new_lines: u64,
    pub new_start: u64,
    pub old_lines: u64,
    pub old_start: u64,
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionStrReplaceResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

pub type ResponseTextEditorCodeExecutionToolResultError = ResponseServerToolResultError<
    TextEditorCodeExecutionToolResultErrorCode,
    TextEditorCodeExecutionToolResultErrorType,
>;

pub type ResponseToolSearchToolResultError =
    ResponseServerToolResultError<ToolSearchToolResultErrorCode, ToolSearchToolResultErrorType>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseServerToolResultError<C, T> {
    pub error_code: C,
    pub error_message: String,
    #[serde(rename = "type")]
    pub type_: T,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolSearchToolSearchResultBlock {
    pub tool_references: Vec<ResponseToolReferenceBlock>,
    #[serde(rename = "type")]
    pub type_: ResponseToolSearchToolSearchResultBlockType,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolSearchToolSearchResultBlockType {
    #[serde(rename = "tool_search_tool_search_result")]
    ToolSearchToolSearchResult,
}
