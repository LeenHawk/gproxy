use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use time::OffsetDateTime;

pub type JsonValue = Value;
pub type JsonObject = BTreeMap<String, JsonValue>;

/// JSON Schema used by structured outputs.
///
/// Limitations and behavior:
/// - Supported: basic types (object/array/string/integer/number/boolean/null),
///   enum (strings/numbers/bools/null only), const, anyOf, allOf (no $ref in
///   allOf), $ref/$defs/definitions (no external $ref), default, required with
///   additionalProperties = false, string formats (date-time, time, date,
///   duration, email, hostname, uri, ipv4, ipv6, uuid), minItems (0 or 1 only).
/// - Not supported: recursive schemas, complex enums, external $ref, numeric
///   constraints (minimum/maximum/multipleOf), string constraints
///   (minLength/maxLength), minItems > 1, additionalProperties != false.
/// - Pattern regex support: ^...$ or partial, quantifiers (* + ? {n,m} small),
///   character classes ([] . \\d \\w \\s), groups (...). Not supported:
///   backrefs (\\1), lookahead/lookbehind, word boundaries (\\b/\\B), large
///   {n,m} ranges.
/// - Errors: unsupported/too complex schemas return 400 (e.g. too many recursive
///   definitions or schema too complex).
/// - Invalid outputs can still happen on refusal or max_tokens cutoff.
/// - Compatibility: works with batch, token counting, streaming, and combined
///   output_format + strict tool use; incompatible with citations and message
///   prefilling.
pub type JsonSchema = Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCacheControlTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaCacheControl {
    Ephemeral {
        #[serde(skip_serializing_if = "Option::is_none")]
        ttl: Option<BetaCacheControlTtl>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCitationsConfigParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaTextCitationParam {
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_char_index: u32,
        start_char_index: u32,
    },
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_page_number: u32,
        start_page_number: u32,
    },
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_block_index: u32,
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
pub enum BetaTextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextBlockParam {
    pub text: String,
    #[serde(rename = "type")]
    pub r#type: BetaTextBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<BetaTextCitationParam>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaImageSource {
    Base64 {
        data: String,
        media_type: BetaImageMediaType,
    },
    Url {
        url: String,
    },
    File {
        file_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaImageBlockType {
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaImageBlockParam {
    pub source: BetaImageSource,
    #[serde(rename = "type")]
    pub r#type: BetaImageBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
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
    Content {
        content: BetaContentBlockSourceContent,
    },
    Url {
        url: String,
    },
    File {
        file_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockSourceContent {
    Text(String),
    Blocks(Vec<BetaContentBlockParam>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaDocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestDocumentBlock {
    pub source: BetaDocumentSource,
    #[serde(rename = "type")]
    pub r#type: BetaDocumentBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaSearchResultBlockType {
    #[serde(rename = "search_result")]
    SearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaSearchResultBlockParam {
    pub content: Vec<BetaTextBlockParam>,
    pub source: String,
    pub title: String,
    #[serde(rename = "type")]
    pub r#type: BetaSearchResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaThinkingBlockType {
    #[serde(rename = "thinking")]
    Thinking,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingBlockParam {
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
pub struct BetaRedactedThinkingBlockParam {
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
pub struct BetaToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
pub struct BetaServerToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: BetaServerToolName,
    #[serde(rename = "type")]
    pub r#type: BetaServerToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    ToolReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolReferenceBlockParam {
    pub tool_name: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolReferenceBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
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
#[serde(untagged)]
pub enum BetaToolResultContent {
    Text(String),
    Blocks(Vec<BetaToolResultContentBlockParam>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolResultBlockType {
    #[serde(rename = "tool_result")]
    ToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlockParam {
    pub encrypted_content: String,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchResultBlockType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchToolRequestErrorType {
    #[serde(rename = "web_search_tool_result_error")]
    WebSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaWebSearchToolRequestError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchToolRequestErrorType,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultContent {
    Results(Vec<BetaWebSearchResultBlockParam>),
    Error(BetaWebSearchToolRequestError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlockParam {
    pub content: BetaWebSearchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaWebSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchResultBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchBlockParam {
    pub content: BetaRequestDocumentBlock,
    pub url: String,
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "time::serde::iso8601::option"
    )]
    pub retrieved_at: Option<OffsetDateTime>,
    #[serde(rename = "type")]
    pub r#type: BetaWebFetchResultBlockType,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaWebFetchToolResultContent {
    WebFetchResult {
        content: BetaRequestDocumentBlock,
        url: String,
        #[serde(
            skip_serializing_if = "Option::is_none",
            with = "time::serde::iso8601::option"
        )]
        retrieved_at: Option<OffsetDateTime>,
    },
    WebFetchToolResultError {
        error_code: BetaWebFetchToolResultErrorCode,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlockParam {
    pub content: BetaWebFetchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaWebFetchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionOutputBlockType {
    #[serde(rename = "code_execution_output")]
    CodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionOutputBlockType,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaCodeExecutionToolResultContent {
    CodeExecutionToolResultError {
        error_code: BetaCodeExecutionToolResultErrorCode,
    },
    CodeExecutionResult {
        content: Vec<BetaCodeExecutionOutputBlockParam>,
        return_code: i32,
        stderr: String,
        stdout: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaCodeExecutionToolResultBlockType {
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlockParam {
    pub content: BetaCodeExecutionToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaCodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionOutputBlockType {
    #[serde(rename = "bash_code_execution_output")]
    BashCodeExecutionOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionOutputBlockType,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaBashCodeExecutionToolResultContent {
    BashCodeExecutionToolResultError {
        error_code: BetaBashCodeExecutionToolResultErrorCode,
    },
    BashCodeExecutionResult {
        content: Vec<BetaBashCodeExecutionOutputBlockParam>,
        return_code: i32,
        stderr: String,
        stdout: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaBashCodeExecutionToolResultBlockType {
    #[serde(rename = "bash_code_execution_tool_result")]
    BashCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlockParam {
    pub content: BetaBashCodeExecutionToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaBashCodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionFileType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaTextEditorCodeExecutionToolResultContent {
    TextEditorCodeExecutionToolResultError {
        error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
        #[serde(skip_serializing_if = "Option::is_none")]
        error_message: Option<String>,
    },
    TextEditorCodeExecutionViewResult {
        content: String,
        file_type: BetaTextEditorCodeExecutionFileType,
        #[serde(skip_serializing_if = "Option::is_none")]
        num_lines: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        start_line: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        total_lines: Option<u32>,
    },
    TextEditorCodeExecutionCreateResult {
        is_file_update: bool,
    },
    TextEditorCodeExecutionStrReplaceResult {
        #[serde(skip_serializing_if = "Option::is_none")]
        lines: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_lines: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        new_start: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        old_lines: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        old_start: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaTextEditorCodeExecutionToolResultBlockType {
    #[serde(rename = "text_editor_code_execution_tool_result")]
    TextEditorCodeExecutionToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlockParam {
    pub content: BetaTextEditorCodeExecutionToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaTextEditorCodeExecutionToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaToolSearchToolResultContent {
    ToolSearchToolResultError {
        error_code: BetaToolSearchToolResultErrorCode,
    },
    ToolSearchToolSearchResult {
        tool_references: Vec<BetaToolReferenceBlockParam>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolResultBlockType {
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlockParam {
    pub content: BetaToolSearchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaToolSearchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaMcpToolUseBlockType {
    #[serde(rename = "mcp_tool_use")]
    McpToolUse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMCPToolUseBlockParam {
    pub id: String,
    pub input: JsonObject,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub r#type: BetaMcpToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMCPToolResultContent {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaMcpToolResultBlockType {
    #[serde(rename = "mcp_tool_result")]
    McpToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMCPToolResultBlockParam {
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaMcpToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<BetaMCPToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaContainerUploadBlockType {
    #[serde(rename = "container_upload")]
    ContainerUpload,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainerUploadBlockParam {
    pub file_id: String,
    #[serde(rename = "type")]
    pub r#type: BetaContainerUploadBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContentBlockParam {
    Text(BetaTextBlockParam),
    Image(BetaImageBlockParam),
    Document(BetaRequestDocumentBlock),
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
    McpToolUse(BetaMCPToolUseBlockParam),
    McpToolResult(BetaRequestMCPToolResultBlockParam),
    ContainerUpload(BetaContainerUploadBlockParam),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BetaMessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaMessageContent {
    Text(String),
    Blocks(Vec<BetaContentBlockParam>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMessageParam {
    pub role: BetaMessageRole,
    pub content: BetaMessageContent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaSystemParam {
    Text(String),
    Blocks(Vec<BetaTextBlockParam>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolInputSchemaType {
    #[serde(rename = "object")]
    Object,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolInputSchema {
    #[serde(rename = "type")]
    pub r#type: BetaToolInputSchemaType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<JsonObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolAllowedCaller {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolCustomType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolCustom {
    pub input_schema: BetaToolInputSchema,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<BetaToolCustomType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolBash {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolCodeExecution {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolComputerUse {
    pub display_height_px: u32,
    pub display_width_px: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_zoom: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolTextEditor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolMemory {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<JsonObject>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaUserLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaUserLocation {
    #[serde(rename = "type")]
    pub r#type: BetaUserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<BetaUserLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_domains: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationsConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_content_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolSearchTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<BetaToolAllowedCaller>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolBm25Type {
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchToolBm2520251119,
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolSearchToolRegexType {
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchToolRegex20251119,
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMCPToolConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMCPToolDefaultConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMCPToolset {
    pub mcp_server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<BetaCacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<BTreeMap<String, BetaMCPToolConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_config: Option<BetaMCPToolDefaultConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTool {
    Custom(BetaToolCustom),
    Builtin(BetaToolBuiltin),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaToolBuiltin {
    #[serde(rename = "bash_20241022")]
    Bash20241022(BetaToolBash),
    #[serde(rename = "bash_20250124")]
    Bash20250124(BetaToolBash),
    #[serde(rename = "code_execution_20250522")]
    CodeExecution20250522(BetaToolCodeExecution),
    #[serde(rename = "code_execution_20250825")]
    CodeExecution20250825(BetaToolCodeExecution),
    #[serde(rename = "computer_20241022")]
    ComputerUse20241022(BetaToolComputerUse),
    #[serde(rename = "computer_20250124")]
    ComputerUse20250124(BetaToolComputerUse),
    #[serde(rename = "computer_20251124")]
    ComputerUse20251124(BetaToolComputerUse),
    #[serde(rename = "text_editor_20241022")]
    TextEditor20241022(BetaToolTextEditor),
    #[serde(rename = "text_editor_20250124")]
    TextEditor20250124(BetaToolTextEditor),
    #[serde(rename = "text_editor_20250429")]
    TextEditor20250429(BetaToolTextEditor),
    #[serde(rename = "text_editor_20250728")]
    TextEditor20250728(BetaToolTextEditor),
    #[serde(rename = "memory_20250818")]
    Memory20250818(BetaToolMemory),
    #[serde(rename = "web_search_20250305")]
    WebSearch20250305(BetaWebSearchTool),
    #[serde(rename = "web_fetch_20250910")]
    WebFetch20250910(BetaWebFetchTool),
    #[serde(rename = "tool_search_tool_bm25_20251119")]
    ToolSearchToolBm2520251119(BetaToolSearchTool),
    #[serde(rename = "tool_search_tool_bm25")]
    ToolSearchToolBm25(BetaToolSearchTool),
    #[serde(rename = "tool_search_tool_regex_20251119")]
    ToolSearchToolRegex20251119(BetaToolSearchTool),
    #[serde(rename = "tool_search_tool_regex")]
    ToolSearchToolRegex(BetaToolSearchTool),
    #[serde(rename = "mcp_toolset")]
    McpToolset(BetaMCPToolset),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaToolChoice {
    Auto {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Any {
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    Tool {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaThinkingConfigParam {
    Enabled {
        /// Must be >= 1024 and less than max_tokens.
        budget_tokens: u32,
    },
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaOutputEffort {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<BetaOutputEffort>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaJSONOutputFormatType {
    #[serde(rename = "json_schema")]
    JsonSchema,
}

/// Requires the `structured-outputs-2025-11-13` beta header.
/// Structured outputs are currently available as a public beta feature in the Claude API for
/// Claude Sonnet 4.5, Claude Opus 4.1, Claude Opus 4.5, and Claude Haiku 4.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaJSONOutputFormat {
    pub schema: JsonSchema,
    #[serde(rename = "type")]
    pub r#type: BetaJSONOutputFormatType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaInputTokensClearAtLeast {
    #[serde(rename = "type")]
    pub r#type: BetaInputTokensClearAtLeastType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaInputTokensClearAtLeastType {
    #[serde(rename = "input_tokens")]
    InputTokens,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaToolUsesKeep {
    #[serde(rename = "type")]
    pub r#type: BetaToolUsesKeepType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaToolUsesKeepType {
    #[serde(rename = "tool_uses")]
    ToolUses,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearToolInputs {
    Bool(bool),
    ToolNames(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaContextManagementTrigger {
    InputTokens { value: u32 },
    ToolUses { value: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingTurns {
    #[serde(rename = "type")]
    pub r#type: BetaThinkingTurnsType,
    pub value: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaThinkingTurnsType {
    #[serde(rename = "thinking_turns")]
    ThinkingTurns,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaAllThinkingTurns {
    #[serde(rename = "type")]
    pub r#type: BetaAllThinkingTurnsType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaAllThinkingTurnsType {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaClearThinkingKeepLiteral {
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaClearThinkingKeep {
    ThinkingTurns(BetaThinkingTurns),
    AllTurns(BetaAllThinkingTurns),
    AllLiteral(BetaClearThinkingKeepLiteral),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaContextManagementEdit {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919 {
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_at_least: Option<BetaInputTokensClearAtLeast>,
        #[serde(skip_serializing_if = "Option::is_none")]
        clear_tool_inputs: Option<BetaClearToolInputs>,
        #[serde(skip_serializing_if = "Option::is_none")]
        exclude_tools: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<BetaToolUsesKeep>,
        #[serde(skip_serializing_if = "Option::is_none")]
        trigger: Option<BetaContextManagementTrigger>,
    },
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015 {
        #[serde(skip_serializing_if = "Option::is_none")]
        keep: Option<BetaClearThinkingKeep>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContextManagementConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<Vec<BetaContextManagementEdit>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BetaRequestMCPServerURLDefinitionType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMCPServerToolConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaRequestMCPServerURLDefinition {
    pub name: String,
    #[serde(rename = "type")]
    pub r#type: BetaRequestMCPServerURLDefinitionType,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_configuration: Option<BetaRequestMCPServerToolConfiguration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelKnown {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Model {
    Known(ModelKnown),
    Custom(String),
}
