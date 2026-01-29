use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use time::OffsetDateTime;

use crate::claude::count_tokens::types::Model;

pub type JsonValue = Value;
pub type JsonObject = BTreeMap<String, JsonValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaSkillType {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaSkillParams {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaSkillType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainerParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<BetaSkillParams>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContainerParam {
    Id(String),
    Params(BetaContainerParams),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaServiceTier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "standard_only")]
    StandardOnly,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainer {
    pub id: String,
    /// RFC 3339 datetime string.
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub skills: Vec<BetaSkill>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaSkill {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaSkillType,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<BetaTextCitation>>,
    pub text: String,
    #[serde(rename = "type")]
    pub r#type: BetaTextBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaTextCitation {
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_char_index: u32,
        file_id: String,
        start_char_index: u32,
    },
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_page_number: u32,
        file_id: String,
        start_page_number: u32,
    },
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_block_index: u32,
        file_id: String,
        start_block_index: u32,
    },
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        title: String,
        url: String,
    },
    SearchResultLocation {
        cited_text: String,
        end_block_index: u32,
        search_result_index: u32,
        source: String,
        start_block_index: u32,
        title: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaThinkingBlockType {
    #[serde(rename = "thinking")]
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingBlock {
    pub signature: String,
    pub thinking: String,
    #[serde(rename = "type")]
    pub r#type: BetaThinkingBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaRedactedThinkingBlockType {
    #[serde(rename = "redacted_thinking")]
    RedactedThinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRedactedThinkingBlock {
    pub data: String,
    #[serde(rename = "type")]
    pub r#type: BetaRedactedThinkingBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaToolCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825 { tool_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolUseBlockType {
    #[serde(rename = "tool_use")]
    ToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaServerToolName {
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "web_fetch")]
    WebFetch,
    #[serde(rename = "code_execution")]
    CodeExecution,
    #[serde(rename = "bash_code_execution")]
    BashCodeExecution,
    #[serde(rename = "text_editor_code_execution")]
    TextEditorCodeExecution,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaServerToolUseBlockType {
    #[serde(rename = "server_tool_use")]
    ServerToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaServerToolUseBlock {
    pub id: String,
    pub caller: BetaToolCaller,
    pub input: JsonObject,
    pub name: BetaServerToolName,
    #[serde(rename = "type")]
    pub r#type: BetaServerToolUseBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlock {
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchResultBlockType,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "max_uses_exceeded")]
    MaxUsesExceeded,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "query_too_long")]
    QueryTooLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultErrorType {
    #[serde(rename = "web_search_tool_result_error")]
    WebSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultBlockContent {
    Error(BetaWebSearchToolResultError),
    Results(Vec<BetaWebSearchResultBlock>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlock {
    pub content: BetaWebSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaPdfMediaType {
    #[serde(rename = "application/pdf")]
    ApplicationPdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextMediaType {
    #[serde(rename = "text/plain")]
    TextPlain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaDocumentSource {
    Base64 {
        data: String,
        media_type: BetaPdfMediaType,
    },
    Text {
        data: String,
        media_type: BetaTextMediaType,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaDocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaDocumentBlock {
    pub citations: BetaCitationConfig,
    pub source: BetaDocumentSource,
    pub title: String,
    #[serde(rename = "type")]
    pub r#type: BetaDocumentBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchBlock {
    pub content: BetaDocumentBlock,
    /// ISO 8601 datetime string.
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "time::serde::iso8601::option"
    )]
    pub retrieved_at: Option<OffsetDateTime>,
    #[serde(rename = "type")]
    pub r#type: BetaWebFetchBlockType,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "url_too_long")]
    UrlTooLong,
    #[serde(rename = "url_not_allowed")]
    UrlNotAllowed,
    #[serde(rename = "url_not_accessible")]
    UrlNotAccessible,
    #[serde(rename = "unsupported_content_type")]
    UnsupportedContentType,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "max_uses_exceeded")]
    MaxUsesExceeded,
    #[serde(rename = "unavailable")]
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultErrorType {
    #[serde(rename = "web_fetch_tool_result_error")]
    WebFetchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlock {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub r#type: BetaWebFetchToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockContent {
    Error(BetaWebFetchToolResultErrorBlock),
    Result(BetaWebFetchBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlock {
    pub content: BetaWebFetchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaWebFetchToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionOutputBlockType {
    #[serde(rename = "code_execution_output")]
    CodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionOutputBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionResultBlockType {
    #[serde(rename = "code_execution_result")]
    CodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionResultBlock {
    pub content: Vec<BetaCodeExecutionOutputBlock>,
    pub return_code: i32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultErrorType {
    #[serde(rename = "code_execution_tool_result_error")]
    CodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultError {
    pub error_code: BetaCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaCodeExecutionToolResultBlockContent {
    Error(BetaCodeExecutionToolResultError),
    Result(BetaCodeExecutionResultBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultBlockType {
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlock {
    pub content: BetaCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionOutputBlockType {
    #[serde(rename = "bash_code_execution_output")]
    BashCodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionOutputBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionResultBlockType {
    #[serde(rename = "bash_code_execution_result")]
    BashCodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionResultBlock {
    pub content: Vec<BetaBashCodeExecutionOutputBlock>,
    pub return_code: i32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
    #[serde(rename = "output_file_too_large")]
    OutputFileTooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultErrorType {
    #[serde(rename = "bash_code_execution_tool_result_error")]
    BashCodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultError {
    pub error_code: BetaBashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaBashCodeExecutionToolResultBlockContent {
    Error(BetaBashCodeExecutionToolResultError),
    Result(BetaBashCodeExecutionResultBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultBlockType {
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlock {
    pub content: BetaBashCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorFileType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionViewResultBlockType {
    #[serde(rename = "text_editor_code_execution_view_result")]
    TextEditorCodeExecutionViewResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: BetaTextEditorFileType,
    pub num_lines: u32,
    pub start_line: u32,
    pub total_lines: u32,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionViewResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionCreateResultBlockType {
    #[serde(rename = "text_editor_code_execution_create_result")]
    TextEditorCodeExecutionCreateResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionCreateResultBlock {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionCreateResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionStrReplaceResultBlockType {
    #[serde(rename = "text_editor_code_execution_str_replace_result")]
    TextEditorCodeExecutionStrReplaceResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionStrReplaceResultBlock {
    pub lines: Vec<String>,
    pub new_lines: u32,
    pub new_start: u32,
    pub old_lines: u32,
    pub old_start: u32,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionStrReplaceResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
    #[serde(rename = "file_not_found")]
    FileNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultErrorType {
    #[serde(rename = "text_editor_code_execution_tool_result_error")]
    TextEditorCodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultError {
    pub error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
    pub error_message: String,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextEditorCodeExecutionToolResultBlockContent {
    Error(BetaTextEditorCodeExecutionToolResultError),
    View(BetaTextEditorCodeExecutionViewResultBlock),
    Create(BetaTextEditorCodeExecutionCreateResultBlock),
    StrReplace(BetaTextEditorCodeExecutionStrReplaceResultBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultBlockType {
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlock {
    pub content: BetaTextEditorCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultErrorCode {
    #[serde(rename = "invalid_tool_input")]
    InvalidToolInput,
    #[serde(rename = "unavailable")]
    Unavailable,
    #[serde(rename = "too_many_requests")]
    TooManyRequests,
    #[serde(rename = "execution_time_exceeded")]
    ExecutionTimeExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultErrorType {
    #[serde(rename = "tool_search_tool_result_error")]
    ToolSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultError {
    pub error_code: BetaToolSearchToolResultErrorCode,
    pub error_message: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolSearchToolResultErrorType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolReferenceBlock {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolReferenceBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolSearchResultBlockType {
    #[serde(rename = "tool_search_tool_search_result")]
    ToolSearchToolSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlock {
    pub tool_references: Vec<BetaToolReferenceBlock>,
    #[serde(rename = "type")]
    pub r#type: BetaToolSearchToolSearchResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockContent {
    Error(BetaToolSearchToolResultError),
    Result(BetaToolSearchToolSearchResultBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultBlockType {
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlock {
    pub content: BetaToolSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolSearchToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaMcpToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    McpToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolUseBlock {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub r#type: BetaMcpToolUseBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMcpToolResultContent {
    Text(String),
    Blocks(Vec<BetaTextBlock>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaMcpToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    McpToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolResultBlock {
    pub content: BetaMcpToolResultContent,
    pub is_error: bool,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaMcpToolResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaContainerUploadBlockType {
    #[serde(rename = "container_upload")]
    ContainerUpload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainerUploadBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaContainerUploadBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlock {
    Text(BetaTextBlock),
    Thinking(BetaThinkingBlock),
    RedactedThinking(BetaRedactedThinkingBlock),
    ToolUse(BetaToolUseBlock),
    ServerToolUse(BetaServerToolUseBlock),
    WebSearchToolResult(BetaWebSearchToolResultBlock),
    WebFetchToolResult(BetaWebFetchToolResultBlock),
    CodeExecutionToolResult(BetaCodeExecutionToolResultBlock),
    BashCodeExecutionToolResult(BetaBashCodeExecutionToolResultBlock),
    TextEditorCodeExecutionToolResult(BetaTextEditorCodeExecutionToolResultBlock),
    ToolSearchToolResult(BetaToolSearchToolResultBlock),
    McpToolUse(BetaMcpToolUseBlock),
    McpToolResult(BetaMcpToolResultBlock),
    ContainerUpload(BetaContainerUploadBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BetaMessageRole {
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaStopReason {
    #[serde(rename = "end_turn")]
    EndTurn,
    #[serde(rename = "max_tokens")]
    MaxTokens,
    #[serde(rename = "stop_sequence")]
    StopSequence,
    #[serde(rename = "tool_use")]
    ToolUse,
    #[serde(rename = "pause_turn")]
    PauseTurn,
    #[serde(rename = "refusal")]
    Refusal,
    #[serde(rename = "model_context_window_exceeded")]
    ModelContextWindowExceeded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCacheCreation {
    pub ephemeral_1h_input_tokens: u32,
    pub ephemeral_5m_input_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaServerToolUsage {
    pub web_fetch_requests: u32,
    pub web_search_requests: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaServiceTierUsed {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "priority")]
    Priority,
    #[serde(rename = "batch")]
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaUsage {
    pub cache_creation: BetaCacheCreation,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<BetaServerToolUsage>,
    pub service_tier: BetaServiceTierUsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaClearToolUsesEditResponseType {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaClearToolUsesEditResponse {
    pub cleared_input_tokens: u32,
    pub cleared_tool_uses: u32,
    #[serde(rename = "type")]
    pub r#type: BetaClearToolUsesEditResponseType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaClearThinkingEditResponseType {
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaClearThinkingEditResponse {
    pub cleared_input_tokens: u32,
    pub cleared_thinking_turns: u32,
    #[serde(rename = "type")]
    pub r#type: BetaClearThinkingEditResponseType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementEditResponse {
    ClearToolUses(BetaClearToolUsesEditResponse),
    ClearThinking(BetaClearThinkingEditResponse),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContextManagementResponse {
    pub applied_edits: Vec<BetaContextManagementEditResponse>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMessage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainer>,
    pub content: Vec<BetaContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementResponse>,
    pub model: Model,
    pub role: BetaMessageRole,
    /// Non-streaming responses include a stop reason; streaming message_start events may be null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub r#type: BetaMessageType,
    pub usage: BetaUsage,
}
