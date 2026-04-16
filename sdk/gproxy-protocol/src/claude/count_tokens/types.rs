use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

pub use crate::claude::types::{
    AnthropicBeta, AnthropicBetaKnown, AnthropicVersion, BetaError, BetaErrorResponse,
    BetaErrorResponseType, BetaErrorType, HttpMethod,
};

/// JSON object used for schema/input maps documented as `map[unknown]`.
pub type JsonObject = BTreeMap<String, Value>;

/// Model parameter for count-tokens endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Model {
    Known(ModelKnown),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelKnown {
    #[serde(rename = "claude-opus-4-7")]
    ClaudeOpus47,
    #[serde(rename = "claude-opus-4-6")]
    ClaudeOpus46,
    #[serde(rename = "claude-opus-4-5-20251101")]
    ClaudeOpus4520251101,
    #[serde(rename = "claude-opus-4-5")]
    ClaudeOpus45,
    #[serde(rename = "claude-3-7-sonnet-latest")]
    Claude37SonnetLatest,
    #[serde(rename = "claude-3-7-sonnet-20250219")]
    Claude37Sonnet20250219,
    #[serde(rename = "claude-3-5-haiku-latest")]
    Claude35HaikuLatest,
    #[serde(rename = "claude-3-5-haiku-20241022")]
    Claude35Haiku20241022,
    #[serde(rename = "claude-haiku-4-5")]
    ClaudeHaiku45,
    #[serde(rename = "claude-haiku-4-5-20251001")]
    ClaudeHaiku4520251001,
    #[serde(rename = "claude-sonnet-4-20250514")]
    ClaudeSonnet420250514,
    #[serde(rename = "claude-sonnet-4-0")]
    ClaudeSonnet40,
    #[serde(rename = "claude-4-sonnet-20250514")]
    Claude4Sonnet20250514,
    #[serde(rename = "claude-sonnet-4-5")]
    ClaudeSonnet45,
    #[serde(rename = "claude-sonnet-4-5-20250929")]
    ClaudeSonnet4520250929,
    #[serde(rename = "claude-sonnet-4-6")]
    ClaudeSonnet46,
    #[serde(rename = "claude-opus-4-0")]
    ClaudeOpus40,
    #[serde(rename = "claude-opus-4-20250514")]
    ClaudeOpus420250514,
    #[serde(rename = "claude-4-opus-20250514")]
    Claude4Opus20250514,
    #[serde(rename = "claude-opus-4-1-20250805")]
    ClaudeOpus4120250805,
    #[serde(rename = "claude-3-opus-latest")]
    Claude3OpusLatest,
    #[serde(rename = "claude-3-opus-20240229")]
    Claude3Opus20240229,
    #[serde(rename = "claude-3-haiku-20240307")]
    Claude3Haiku20240307,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMessageParam {
    pub content: BetaMessageContent,
    pub role: BetaMessageRole,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMessageContent {
    Text(String),
    Blocks(Vec<BetaContentBlockParam>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockParam {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
    RequestDocument(BetaRequestDocumentBlock),
    SearchResult(BetaSearchResultBlockParam),
    Thinking(BetaThinkingBlockParam),
    RedactedThinking(BetaRedactedThinkingBlockParam),
    ToolUse(BetaToolUseBlockParam),
    ToolResult(BetaToolResultBlockParam),
    ServerToolUse(BetaServerToolUseBlockParam),
    WebSearchToolResult(BetaWebSearchToolResultBlockParam),
    WebFetchToolResult(BetaWebFetchToolResultBlockParam),
    CodeExecutionToolResult(BetaCodeExecutionToolResultBlockParam),
    BashCodeExecutionToolResult(BetaBashCodeExecutionToolResultBlockParam),
    TextEditorCodeExecutionToolResult(BetaTextEditorCodeExecutionToolResultBlockParam),
    ToolSearchToolResult(BetaToolSearchToolResultBlockParam),
    McpToolUse(BetaMcpToolUseBlockParam),
    McpToolResult(BetaRequestMcpToolResultBlockParam),
    ContainerUpload(BetaContainerUploadBlockParam),
    Compaction(BetaCompactionBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextBlockParam {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: BetaTextBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<BetaTextCitationParam>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaImageBlockParam {
    pub source: BetaImageSource,
    #[serde(rename = "type")]
    pub type_: BetaImageBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaImageBlockType {
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaImageSource {
    Base64(BetaBase64ImageSource),
    Url(BetaUrlImageSource),
    File(BetaFileImageSource),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBase64ImageSource {
    pub data: String,
    pub media_type: BetaImageMediaType,
    #[serde(rename = "type")]
    pub type_: BetaBase64SourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaImageMediaType {
    #[serde(rename = "image/jpeg")]
    ImageJpeg,
    #[serde(rename = "image/png")]
    ImagePng,
    #[serde(rename = "image/gif")]
    ImageGif,
    #[serde(rename = "image/webp")]
    ImageWebp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBase64SourceType {
    #[serde(rename = "base64")]
    Base64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaUrlImageSource {
    #[serde(rename = "type")]
    pub type_: BetaUrlSourceType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaUrlSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaFileImageSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BetaFileSourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaFileSourceType {
    #[serde(rename = "file")]
    File,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestDocumentBlock {
    pub source: BetaDocumentSource,
    #[serde(rename = "type")]
    pub type_: BetaRequestDocumentBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaRequestDocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaDocumentSource {
    Base64Pdf(BetaBase64PdfSource),
    PlainText(BetaPlainTextSource),
    Content(BetaContentBlockSource),
    UrlPdf(BetaUrlPdfSource),
    File(BetaFileDocumentSource),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBase64PdfSource {
    pub data: String,
    pub media_type: BetaPdfMediaType,
    #[serde(rename = "type")]
    pub type_: BetaBase64SourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaPdfMediaType {
    #[serde(rename = "application/pdf")]
    ApplicationPdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaPlainTextSource {
    pub data: String,
    pub media_type: BetaPlainTextMediaType,
    #[serde(rename = "type")]
    pub type_: BetaTextSourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaPlainTextMediaType {
    #[serde(rename = "text/plain")]
    TextPlain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextSourceType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContentBlockSource {
    pub content: BetaContentBlockSourceContentPayload,
    #[serde(rename = "type")]
    pub type_: BetaContentBlockSourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaContentBlockSourceType {
    #[serde(rename = "content")]
    Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockSourceContentPayload {
    Text(String),
    Blocks(Vec<BetaContentBlockSourceContent>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockSourceContent {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaUrlPdfSource {
    #[serde(rename = "type")]
    pub type_: BetaUrlSourceType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaFileDocumentSource {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BetaFileSourceType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaSearchResultBlockParam {
    pub content: Vec<BetaTextBlockParam>,
    pub source: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaSearchResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaSearchResultBlockType {
    #[serde(rename = "search_result")]
    SearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingBlockParam {
    pub signature: String,
    pub thinking: String,
    #[serde(rename = "type")]
    pub type_: BetaThinkingBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaThinkingBlockType {
    #[serde(rename = "thinking")]
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRedactedThinkingBlockParam {
    pub data: String,
    #[serde(rename = "type")]
    pub type_: BetaRedactedThinkingBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaRedactedThinkingBlockType {
    #[serde(rename = "redacted_thinking")]
    RedactedThinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: BetaToolUseBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolUseBlockType {
    #[serde(rename = "tool_use")]
    ToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolCaller {
    Direct(BetaDirectCaller),
    Server(BetaServerToolCaller),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaDirectCaller {
    #[serde(rename = "type")]
    pub type_: BetaDirectCallerType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaDirectCallerType {
    #[serde(rename = "direct")]
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaServerToolCaller {
    pub tool_id: String,
    #[serde(rename = "type")]
    pub type_: BetaServerToolCallerType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaServerToolCallerType {
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaToolResultBlockParamContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolResultBlockType {
    #[serde(rename = "tool_result")]
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolResultBlockParamContent {
    Text(String),
    Blocks(Vec<BetaToolResultContentBlockParam>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolResultContentBlockParam {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
    SearchResult(BetaSearchResultBlockParam),
    Document(BetaRequestDocumentBlock),
    ToolReference(BetaToolReferenceBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolReferenceBlockParam {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub type_: BetaToolReferenceBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaServerToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: BetaServerToolUseName,
    #[serde(rename = "type")]
    pub type_: BetaServerToolUseBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaServerToolUseName {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaServerToolUseBlockType {
    #[serde(rename = "server_tool_use")]
    ServerToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlockParam {
    pub content: BetaWebSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultBlockParamContent {
    Results(Vec<BetaWebSearchResultBlockParam>),
    Error(BetaWebSearchToolRequestError),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlockParam {
    pub encrypted_content: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchResultBlockType,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolRequestError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchToolRequestErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchToolRequestErrorType {
    #[serde(rename = "web_search_tool_result_error")]
    WebSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(rename = "request_too_large")]
    RequestTooLarge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlockParam {
    pub content: BetaWebFetchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockParamContent {
    Error(BetaWebFetchToolResultErrorBlockParam),
    Result(BetaWebFetchBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlockParam {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchToolResultErrorBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultErrorBlockType {
    #[serde(rename = "web_fetch_tool_result_error")]
    WebFetchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchBlockParam {
    pub content: BetaRequestDocumentBlock,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchBlockType,
    pub url: String,
    #[serde(
        default,
        with = "time::serde::rfc3339::option",
        skip_serializing_if = "Option::is_none"
    )]
    pub retrieved_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebFetchBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlockParam {
    pub content: BetaCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultBlockType {
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaCodeExecutionToolResultBlockParamContent {
    Error(BetaCodeExecutionToolResultErrorParam),
    Result(BetaCodeExecutionResultBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultErrorParam {
    pub error_code: BetaCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultErrorType {
    #[serde(rename = "code_execution_tool_result_error")]
    CodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionResultBlockParam {
    pub content: Vec<BetaCodeExecutionOutputBlockParam>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionResultBlockType {
    #[serde(rename = "code_execution_result")]
    CodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionOutputBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionOutputBlockType {
    #[serde(rename = "code_execution_output")]
    CodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlockParam {
    pub content: BetaBashCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaBashCodeExecutionToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultBlockType {
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaBashCodeExecutionToolResultBlockParamContent {
    Error(BetaBashCodeExecutionToolResultErrorParam),
    Result(BetaBashCodeExecutionResultBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultErrorParam {
    pub error_code: BetaBashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaBashCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultErrorType {
    #[serde(rename = "bash_code_execution_tool_result_error")]
    BashCodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionResultBlockParam {
    pub content: Vec<BetaBashCodeExecutionOutputBlockParam>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub type_: BetaBashCodeExecutionResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionResultBlockType {
    #[serde(rename = "bash_code_execution_result")]
    BashCodeExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BetaBashCodeExecutionOutputBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionOutputBlockType {
    #[serde(rename = "bash_code_execution_output")]
    BashCodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlockParam {
    pub content: BetaTextEditorCodeExecutionToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaTextEditorCodeExecutionToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultBlockType {
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextEditorCodeExecutionToolResultBlockParamContent {
    Error(BetaTextEditorCodeExecutionToolResultErrorParam),
    View(BetaTextEditorCodeExecutionViewResultBlockParam),
    Create(BetaTextEditorCodeExecutionCreateResultBlockParam),
    StrReplace(BetaTextEditorCodeExecutionStrReplaceResultBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultErrorParam {
    pub error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaTextEditorCodeExecutionToolResultErrorType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultErrorType {
    #[serde(rename = "text_editor_code_execution_tool_result_error")]
    TextEditorCodeExecutionToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionViewResultBlockParam {
    pub content: String,
    pub file_type: BetaTextEditorFileType,
    #[serde(rename = "type")]
    pub type_: BetaTextEditorCodeExecutionViewResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_lines: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorFileType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionViewResultBlockType {
    #[serde(rename = "text_editor_code_execution_view_result")]
    TextEditorCodeExecutionViewResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionCreateResultBlockParam {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub type_: BetaTextEditorCodeExecutionCreateResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionCreateResultBlockType {
    #[serde(rename = "text_editor_code_execution_create_result")]
    TextEditorCodeExecutionCreateResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionStrReplaceResultBlockParam {
    #[serde(rename = "type")]
    pub type_: BetaTextEditorCodeExecutionStrReplaceResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_lines: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_start: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_lines: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_start: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionStrReplaceResultBlockType {
    #[serde(rename = "text_editor_code_execution_str_replace_result")]
    TextEditorCodeExecutionStrReplaceResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlockParam {
    pub content: BetaToolSearchToolResultBlockParamContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaToolSearchToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultBlockType {
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockParamContent {
    Error(BetaToolSearchToolResultErrorParam),
    Result(BetaToolSearchToolSearchResultBlockParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultErrorParam {
    pub error_code: BetaToolSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaToolSearchToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultErrorType {
    #[serde(rename = "tool_search_tool_result_error")]
    ToolSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlockParam {
    pub tool_references: Vec<BetaToolReferenceBlockParam>,
    #[serde(rename = "type")]
    pub type_: BetaToolSearchToolSearchResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolSearchResultBlockType {
    #[serde(rename = "tool_search_tool_search_result")]
    ToolSearchToolSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: BetaMcpToolUseBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaMcpToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    McpToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMcpToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaRequestMcpToolResultBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaMcpToolResultBlockParamContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaRequestMcpToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    McpToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMcpToolResultBlockParamContent {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainerUploadBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub type_: BetaContainerUploadBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaContainerUploadBlockType {
    #[serde(rename = "container_upload")]
    ContainerUpload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCompactionBlockParam {
    pub content: Option<String>,
    #[serde(rename = "type")]
    pub type_: BetaCompactionBlockType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCompactionBlockType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCacheControlEphemeral {
    #[serde(rename = "type")]
    pub type_: BetaCacheControlType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<BetaCacheControlTtl>,
}

impl Default for BetaCacheControlEphemeral {
    fn default() -> Self {
        Self {
            type_: BetaCacheControlType::Ephemeral,
            ttl: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCacheControlType {
    #[serde(rename = "ephemeral")]
    Ephemeral,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCacheControlTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationsConfigParam {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextCitationParam {
    CharLocation(BetaCitationCharLocationParam),
    PageLocation(BetaCitationPageLocationParam),
    ContentBlockLocation(BetaCitationContentBlockLocationParam),
    WebSearchResultLocation(BetaCitationWebSearchResultLocationParam),
    SearchResultLocation(BetaCitationSearchResultLocationParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationCharLocationParam {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_char_index: u64,
    pub start_char_index: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationCharLocationType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCitationCharLocationType {
    #[serde(rename = "char_location")]
    CharLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationPageLocationParam {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_page_number: u64,
    pub start_page_number: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationPageLocationType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCitationPageLocationType {
    #[serde(rename = "page_location")]
    PageLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationContentBlockLocationParam {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_block_index: u64,
    pub start_block_index: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationContentBlockLocationType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCitationContentBlockLocationType {
    #[serde(rename = "content_block_location")]
    ContentBlockLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationWebSearchResultLocationParam {
    pub cited_text: String,
    pub encrypted_index: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaCitationWebSearchResultLocationType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCitationWebSearchResultLocationType {
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationSearchResultLocationParam {
    pub cited_text: String,
    pub end_block_index: u64,
    pub search_result_index: u64,
    pub source: String,
    pub start_block_index: u64,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaCitationSearchResultLocationType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCitationSearchResultLocationType {
    #[serde(rename = "search_result_location")]
    SearchResultLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContextManagementConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<BetaContextManagementEdit>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementEdit {
    ClearToolUses(BetaClearToolUses20250919Edit),
    ClearThinking(BetaClearThinking20251015Edit),
    Compact(BetaCompact20260112Edit),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919Edit {
    #[serde(rename = "type")]
    pub type_: BetaClearToolUsesType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_at_least: Option<BetaInputTokensClearAtLeast>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clear_tool_inputs: Option<BetaClearToolInputs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<BetaToolUsesKeep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<BetaClearToolUsesTrigger>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaClearToolUsesType {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearToolInputs {
    All(bool),
    Selected(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearToolUsesTrigger {
    InputTokens(BetaInputTokensTrigger),
    ToolUses(BetaToolUsesTrigger),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaInputTokensClearAtLeast {
    #[serde(rename = "type")]
    pub type_: BetaInputTokensCounterType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaInputTokensTrigger {
    #[serde(rename = "type")]
    pub type_: BetaInputTokensCounterType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaInputTokensCounterType {
    #[serde(rename = "input_tokens")]
    InputTokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolUsesKeep {
    #[serde(rename = "type")]
    pub type_: BetaToolUsesCounterType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolUsesTrigger {
    #[serde(rename = "type")]
    pub type_: BetaToolUsesCounterType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolUsesCounterType {
    #[serde(rename = "tool_uses")]
    ToolUses,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaClearThinking20251015Edit {
    #[serde(rename = "type")]
    pub type_: BetaClearThinkingType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<BetaClearThinkingKeep>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaClearThinkingType {
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearThinkingKeep {
    ThinkingTurns(BetaThinkingTurns),
    AllThinkingTurns(BetaAllThinkingTurns),
    All(BetaAllLiteral),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingTurns {
    #[serde(rename = "type")]
    pub type_: BetaThinkingTurnsType,
    pub value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaThinkingTurnsType {
    #[serde(rename = "thinking_turns")]
    ThinkingTurns,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaAllThinkingTurns {
    #[serde(rename = "type")]
    pub type_: BetaAllType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaAllType {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaAllLiteral {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCompact20260112Edit {
    #[serde(rename = "type")]
    pub type_: BetaCompactType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_after_compaction: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<BetaInputTokensTrigger>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCompactType {
    #[serde(rename = "compact_20260112")]
    Compact20260112,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMcpServerUrlDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: BetaRequestMcpServerType,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<BetaRequestMcpServerToolConfiguration>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaRequestMcpServerType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMcpServerToolConfiguration {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolDefaultConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaOutputConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<BetaOutputEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<BetaJsonOutputFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_budget: Option<BetaTaskBudget>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaOutputEffort {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "xhigh")]
    XHigh,
    #[serde(rename = "max")]
    Max,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTaskBudget {
    pub total: u64,
    #[serde(rename = "type")]
    pub type_: BetaTaskBudgetType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaTaskBudgetType {
    #[serde(rename = "tokens")]
    Tokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaJsonOutputFormat {
    pub schema: JsonObject,
    #[serde(rename = "type")]
    pub type_: BetaJsonOutputFormatType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaJsonOutputFormatType {
    #[serde(rename = "json_schema")]
    JsonSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaSystemPrompt {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaThinkingConfigParam {
    Enabled(BetaThinkingConfigEnabled),
    Disabled(BetaThinkingConfigDisabled),
    Adaptive(BetaThinkingConfigAdaptive),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingConfigEnabled {
    pub budget_tokens: u64,
    #[serde(rename = "type")]
    pub type_: BetaThinkingConfigEnabledType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<BetaThinkingDisplay>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaThinkingConfigEnabledType {
    #[serde(rename = "enabled")]
    Enabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingConfigDisabled {
    #[serde(rename = "type")]
    pub type_: BetaThinkingConfigDisabledType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaThinkingConfigDisabledType {
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingConfigAdaptive {
    #[serde(rename = "type")]
    pub type_: BetaThinkingConfigAdaptiveType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<BetaThinkingDisplay>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaThinkingConfigAdaptiveType {
    #[serde(rename = "adaptive")]
    Adaptive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaThinkingDisplay {
    #[serde(rename = "summarized")]
    Summarized,
    #[serde(rename = "omitted")]
    Omitted,
}

#[cfg(test)]
mod tests {
    use super::{
        BetaOutputConfig, BetaOutputEffort, BetaTaskBudget, BetaTaskBudgetType, Model, ModelKnown,
    };

    #[test]
    fn model_round_trips_claude_opus_47() {
        let model: Model = serde_json::from_str(r#""claude-opus-4-7""#).expect("model");
        assert_eq!(model, Model::Known(ModelKnown::ClaudeOpus47));
        assert_eq!(
            serde_json::to_string(&model).expect("serialize model"),
            r#""claude-opus-4-7""#
        );
    }

    #[test]
    fn output_config_serializes_xhigh_and_task_budget() {
        let output_config = BetaOutputConfig {
            effort: Some(BetaOutputEffort::XHigh),
            format: None,
            task_budget: Some(BetaTaskBudget {
                total: 128_000,
                type_: BetaTaskBudgetType::Tokens,
            }),
        };

        let value = serde_json::to_value(output_config).expect("serialize output config");
        assert_eq!(value["effort"], "xhigh");
        assert_eq!(value["task_budget"]["type"], "tokens");
        assert_eq!(value["task_budget"]["total"], 128_000);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolChoice {
    Auto(BetaToolChoiceAuto),
    Any(BetaToolChoiceAny),
    Tool(BetaToolChoiceTool),
    None(BetaToolChoiceNone),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolChoiceAuto {
    #[serde(rename = "type")]
    pub type_: BetaToolChoiceAutoType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolChoiceAutoType {
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolChoiceAny {
    #[serde(rename = "type")]
    pub type_: BetaToolChoiceAnyType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolChoiceAnyType {
    #[serde(rename = "any")]
    Any,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolChoiceTool {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: BetaToolChoiceToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_parallel_tool_use: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolChoiceToolType {
    #[serde(rename = "tool")]
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolChoiceNone {
    #[serde(rename = "type")]
    pub type_: BetaToolChoiceNoneType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolChoiceNoneType {
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolUnion {
    Custom(BetaTool),
    Bash20241022(BetaToolBash20241022),
    Bash20250124(BetaToolBash20250124),
    CodeExecution20250522(BetaCodeExecutionTool20250522),
    CodeExecution20250825(BetaCodeExecutionTool20250825),
    ComputerUse20241022(BetaToolComputerUse20241022),
    Memory20250818(BetaMemoryTool20250818),
    ComputerUse20250124(BetaToolComputerUse20250124),
    TextEditor20241022(BetaToolTextEditor20241022),
    ComputerUse20251124(BetaToolComputerUse20251124),
    TextEditor20250124(BetaToolTextEditor20250124),
    TextEditor20250429(BetaToolTextEditor20250429),
    TextEditor20250728(BetaToolTextEditor20250728),
    WebSearch20250305(BetaWebSearchTool20250305),
    WebFetch20250910(BetaWebFetchTool20250910),
    ToolSearchBm25_20251119(BetaToolSearchToolBm25_20251119),
    ToolSearchRegex20251119(BetaToolSearchToolRegex20251119),
    McpToolset(BetaMcpToolset),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolAllowedCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BetaToolCommonFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTool {
    pub input_schema: BetaToolInputSchema,
    pub name: String,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eager_input_streaming: Option<bool>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<BetaCustomToolType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCustomToolType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolInputSchema {
    #[serde(rename = "type")]
    pub type_: BetaToolInputSchemaType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<JsonObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
    #[serde(
        flatten,
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
    pub extra_fields: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolInputSchemaType {
    #[serde(rename = "object")]
    Object,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolBash20241022 {
    pub name: BetaBashToolName,
    #[serde(rename = "type")]
    pub type_: BetaToolBash20241022Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaBashToolName {
    #[serde(rename = "bash")]
    Bash,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolBash20241022Type {
    #[serde(rename = "bash_20241022")]
    Bash20241022,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolBash20250124 {
    pub name: BetaBashToolName,
    #[serde(rename = "type")]
    pub type_: BetaToolBash20250124Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolBash20250124Type {
    #[serde(rename = "bash_20250124")]
    Bash20250124,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250522 {
    pub name: BetaCodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionTool20250522Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolName {
    #[serde(rename = "code_execution")]
    CodeExecution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionTool20250522Type {
    #[serde(rename = "code_execution_20250522")]
    CodeExecution20250522,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionTool20250825 {
    pub name: BetaCodeExecutionToolName,
    #[serde(rename = "type")]
    pub type_: BetaCodeExecutionTool20250825Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaCodeExecutionTool20250825Type {
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolComputerUse20241022 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: BetaComputerToolName,
    #[serde(rename = "type")]
    pub type_: BetaToolComputerUse20241022Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaComputerToolName {
    #[serde(rename = "computer")]
    Computer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolComputerUse20241022Type {
    #[serde(rename = "computer_20241022")]
    Computer20241022,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMemoryTool20250818 {
    pub name: BetaMemoryToolName,
    #[serde(rename = "type")]
    pub type_: BetaMemoryTool20250818Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaMemoryToolName {
    #[serde(rename = "memory")]
    Memory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaMemoryTool20250818Type {
    #[serde(rename = "memory_20250818")]
    Memory20250818,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolComputerUse20250124 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: BetaComputerToolName,
    #[serde(rename = "type")]
    pub type_: BetaToolComputerUse20250124Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolComputerUse20250124Type {
    #[serde(rename = "computer_20250124")]
    Computer20250124,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolTextEditor20241022 {
    pub name: BetaTextEditorToolNameV1,
    #[serde(rename = "type")]
    pub type_: BetaToolTextEditor20241022Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorToolNameV1 {
    #[serde(rename = "str_replace_editor")]
    StrReplaceEditor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolTextEditor20241022Type {
    #[serde(rename = "text_editor_20241022")]
    TextEditor20241022,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolComputerUse20251124 {
    pub display_height_px: u64,
    pub display_width_px: u64,
    pub name: BetaComputerToolName,
    #[serde(rename = "type")]
    pub type_: BetaToolComputerUse20251124Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolComputerUse20251124Type {
    #[serde(rename = "computer_20251124")]
    Computer20251124,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250124 {
    pub name: BetaTextEditorToolNameV1,
    #[serde(rename = "type")]
    pub type_: BetaToolTextEditor20250124Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolTextEditor20250124Type {
    #[serde(rename = "text_editor_20250124")]
    TextEditor20250124,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250429 {
    pub name: BetaTextEditorToolNameV2,
    #[serde(rename = "type")]
    pub type_: BetaToolTextEditor20250429Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaTextEditorToolNameV2 {
    #[serde(rename = "str_replace_based_edit_tool")]
    StrReplaceBasedEditTool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolTextEditor20250429Type {
    #[serde(rename = "text_editor_20250429")]
    TextEditor20250429,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolTextEditor20250728 {
    pub name: BetaTextEditorToolNameV2,
    #[serde(rename = "type")]
    pub type_: BetaToolTextEditor20250728Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolTextEditor20250728Type {
    #[serde(rename = "text_editor_20250728")]
    TextEditor20250728,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchTool20250305 {
    pub name: BetaWebSearchToolName,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchTool20250305Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<BetaWebSearchUserLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchToolName {
    #[serde(rename = "web_search")]
    WebSearch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchTool20250305Type {
    #[serde(rename = "web_search_20250305")]
    WebSearch20250305,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchUserLocation {
    #[serde(rename = "type")]
    pub type_: BetaWebSearchUserLocationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebSearchUserLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchTool20250910 {
    pub name: BetaWebFetchToolName,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchTool20250910Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebFetchToolName {
    #[serde(rename = "web_fetch")]
    WebFetch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaWebFetchTool20250910Type {
    #[serde(rename = "web_fetch_20250910")]
    WebFetch20250910,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolBm25_20251119 {
    pub name: BetaToolSearchToolBm25Name,
    #[serde(rename = "type")]
    pub type_: BetaToolSearchToolBm25Type,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolBm25Name {
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolBm25Type {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchToolBm2520251119,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolRegex20251119 {
    pub name: BetaToolSearchToolRegexName,
    #[serde(rename = "type")]
    pub type_: BetaToolSearchToolRegexType,
    #[serde(flatten)]
    pub common: BetaToolCommonFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolRegexName {
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaToolSearchToolRegexType {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchToolRegex20251119,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolset {
    pub mcp_server_name: String,
    #[serde(rename = "type")]
    pub type_: BetaMcpToolsetType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControlEphemeral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configs: Option<BTreeMap<String, BetaMcpToolConfig>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_config: Option<BetaMcpToolDefaultConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BetaMcpToolsetType {
    #[serde(rename = "mcp_toolset")]
    McpToolset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMcpToolConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCountTokensContextManagementResponse {
    pub original_input_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMessageTokensCount {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaCountTokensContextManagementResponse>,
    pub input_tokens: u64,
}
