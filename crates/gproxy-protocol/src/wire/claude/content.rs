use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Rest;

fn present_value<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Value::deserialize(deserializer).map(Some)
}

fn present_nullable_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Message {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clear_at: Option<SystemMessageClearAt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<SystemMessageOutputConfig>,

    pub role: Role,
    pub content: MessageContent,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text(TextBlock),
    #[serde(rename = "image")]
    Image(ImageBlock),
    #[serde(rename = "document")]
    Document(DocumentBlock),
    #[serde(rename = "thinking")]
    Thinking(ThinkingBlock),
    #[serde(rename = "redacted_thinking")]
    RedactedThinking(RedactedThinkingBlock),
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseBlock),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultBlock),
    #[serde(rename = "server_tool_use")]
    ServerToolUse(ServerToolUseBlock),
    #[serde(rename = "search_result")]
    SearchResult(SearchResultBlock),
    WebSearchToolResult(WebSearchToolResultBlock),
    WebFetchToolResult(WebFetchToolResultBlock),
    AdvisorToolResult(AdvisorToolResultBlock),
    CodeExecutionToolResult(CodeExecutionToolResultBlock),
    BashCodeExecutionToolResult(BashCodeExecutionToolResultBlock),
    TextEditorCodeExecutionToolResult(TextEditorCodeExecutionToolResultBlock),
    ToolSearchToolResult(ToolSearchToolResultBlock),
    McpToolListing(McpToolListingBlock),
    McpToolUse(McpToolUseBlock),
    McpToolResult(McpToolResultBlock),
    ContainerUpload(ContainerUploadBlock),
    Compaction(CompactionBlock),
    MidConversationSystem(MidConversationSystemBlock),
    ToolAddition(ToolAdditionBlock),
    ToolRemoval(ToolRemovalBlock),
    Fallback(FallbackBlock),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub type_: CacheControlType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CacheControlType {
    Ephemeral,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CacheTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextBlock {
    #[serde(rename = "type")]
    pub type_: TextBlockType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<TextCitation>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextCitation {
    #[serde(rename = "char_location")]
    Char(CharCitation),
    #[serde(rename = "page_location")]
    Page(PageCitation),
    #[serde(rename = "content_block_location")]
    ContentBlock(ContentBlockCitation),
    #[serde(rename = "web_search_result_location")]
    Web(WebCitation),
    #[serde(rename = "search_result_location")]
    Search(SearchCitation),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CharCitation {
    pub cited_text: String,
    pub document_index: i64,
    pub document_title: String,
    pub end_char_index: i64,
    pub start_char_index: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PageCitation {
    pub cited_text: String,
    pub document_index: i64,
    pub document_title: String,
    pub end_page_number: i64,
    pub start_page_number: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContentBlockCitation {
    pub cited_text: String,
    pub document_index: i64,
    pub document_title: String,
    pub end_block_index: i64,
    pub start_block_index: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebCitation {
    pub cited_text: String,
    pub encrypted_index: String,
    pub title: String,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SearchCitation {
    pub cited_text: String,
    pub end_block_index: i64,
    pub search_result_index: i64,
    pub source: String,
    pub start_block_index: i64,
    pub title: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ImageBlock {
    #[serde(rename = "type")]
    pub type_: ImageBlockType,
    pub source: ImageSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageSource {
    #[serde(rename = "base64")]
    Base64(Base64Source),
    #[serde(rename = "url")]
    Url(UrlSource),
    #[serde(rename = "file")]
    File(FileSource),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct Base64Source {
    pub data: String,
    pub media_type: ImageMediaType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct UrlSource {
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FileSource {
    pub file_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageMediaType {
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct DocumentBlock {
    #[serde(rename = "type")]
    pub type_: DocumentBlockType,
    pub source: DocumentSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum DocumentSource {
    #[serde(rename = "base64")]
    Base64(PdfBase64Source),
    #[serde(rename = "text")]
    Text(PlainTextSource),
    #[serde(rename = "url")]
    Url(UrlSource),
    #[serde(rename = "file")]
    File(FileSource),
    #[serde(rename = "content")]
    Content(ContentSource),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PdfBase64Source {
    pub data: String,
    pub media_type: PdfMediaType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PlainTextSource {
    pub data: String,
    pub media_type: TextMediaType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContentSource {
    pub content: DocumentContent,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum PdfMediaType {
    #[serde(rename = "application/pdf")]
    Pdf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextMediaType {
    #[serde(rename = "text/plain")]
    Plain,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CitationsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ThinkingBlock {
    #[serde(rename = "type")]
    pub type_: ThinkingBlockType,
    pub signature: String,
    pub thinking: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RedactedThinkingBlock {
    #[serde(rename = "type")]
    pub type_: RedactedThinkingBlockType,
    pub data: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolUseBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolset_name: Option<String>,

    #[serde(rename = "type")]
    pub type_: ToolUseBlockType,
    pub id: String,
    pub input: Value,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<ToolCaller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolResultBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolset_name: Option<String>,

    #[serde(rename = "type")]
    pub type_: ToolResultBlockType,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<ToolResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultContentBlock>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolResultContentBlock {
    BrowserState(BrowserStateBlock),
    Text(TextBlock),
    Image(ImageBlock),
    SearchResult(SearchResultBlock),
    Document(DocumentBlock),
    ToolReference(ToolReferenceBlock),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum DocumentContent {
    Text(String),
    Blocks(Vec<DocumentContentBlock>),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum DocumentContentBlock {
    Text(TextBlock),
    Image(ImageBlock),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ServerToolUseBlock {
    #[serde(rename = "type")]
    pub type_: ServerToolUseBlockType,
    pub id: String,
    pub input: Value,
    pub name: ServerToolName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<ToolCaller>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ServerToolName {
    Advisor,
    WebSearch,
    WebFetch,
    CodeExecution,
    BashCodeExecution,
    TextEditorCodeExecution,
    ToolSearchToolRegex,
    ToolSearchToolBm25,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SearchResultBlock {
    #[serde(rename = "type")]
    pub type_: SearchResultBlockType,
    pub content: Vec<TextBlock>,
    pub source: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationsConfig>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolReferenceBlock {
    #[serde(rename = "type")]
    pub type_: ToolReferenceBlockType,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchToolResultType {
    #[serde(rename = "web_search_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchToolResultBlock {
    #[serde(rename = "type")]
    pub type_: WebSearchToolResultType,
    pub content: WebSearchResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<ToolCaller>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchToolResultBlock {
    #[serde(rename = "type")]
    pub type_: WebFetchToolResultType,
    pub content: WebFetchResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<ToolCaller>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum WebFetchToolResultType {
    #[serde(rename = "web_fetch_tool_result")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AdvisorToolResultType {
    #[serde(rename = "advisor_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AdvisorToolResultBlock {
    #[serde(rename = "type")]
    pub type_: AdvisorToolResultType,
    pub content: AdvisorResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionToolResultType {
    #[serde(rename = "code_execution_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionToolResultBlock {
    #[serde(rename = "type")]
    pub type_: CodeExecutionToolResultType,
    pub content: CodeExecutionResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashCodeExecutionToolResultType {
    #[serde(rename = "bash_code_execution_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashCodeExecutionToolResultBlock {
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionToolResultType,
    pub content: BashCodeExecutionResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorCodeExecutionToolResultType {
    #[serde(rename = "text_editor_code_execution_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorCodeExecutionToolResultBlock {
    #[serde(rename = "type")]
    pub type_: TextEditorCodeExecutionToolResultType,
    pub content: TextEditorResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchToolResultType {
    #[serde(rename = "tool_search_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolSearchToolResultBlock {
    #[serde(rename = "type")]
    pub type_: ToolSearchToolResultType,
    pub content: ToolSearchResultContent,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpToolUseType {
    #[serde(rename = "mcp_tool_use")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolUseBlock {
    #[serde(rename = "type")]
    pub type_: McpToolUseType,
    pub id: String,
    pub input: Value,
    pub name: String,
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpToolResultType {
    #[serde(rename = "mcp_tool_result")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolResultBlock {
    #[serde(rename = "type")]
    pub type_: McpToolResultType,
    pub tool_use_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<McpResultContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ContainerUploadType {
    #[serde(rename = "container_upload")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContainerUploadBlock {
    #[serde(rename = "type")]
    pub type_: ContainerUploadType,
    pub file_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CompactionBlock {
    #[serde(
        default,
        deserialize_with = "present_nullable_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub signature: Option<Option<String>>,

    #[serde(rename = "type")]
    pub type_: CompactionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(
        default,
        deserialize_with = "present_nullable_string",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
pub enum CompactionType {
    #[serde(rename = "compaction")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MidConversationSystemType {
    #[serde(rename = "mid_conv_system")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MidConversationSystemBlock {
    #[serde(rename = "type")]
    pub type_: MidConversationSystemType,
    pub content: Vec<MidConversationContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolAdditionType {
    #[serde(rename = "tool_addition")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolAdditionBlock {
    #[serde(rename = "type")]
    pub type_: ToolAdditionType,
    pub tool: ToolAdditionReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolRemovalType {
    #[serde(rename = "tool_removal")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolRemovalBlock {
    #[serde(rename = "type")]
    pub type_: ToolRemovalType,
    pub tool: ToolChangeReference,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum FallbackType {
    #[serde(rename = "fallback")]
    Tag,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FallbackBlock {
    #[serde(rename = "type")]
    pub type_: FallbackType,
    pub from: FallbackInfo,
    pub to: FallbackInfo,
    #[serde(
        default,
        deserialize_with = "present_value",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger: Option<Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchResultContent {
    Results(Vec<WebSearchResult>),
    Error(WebSearchError),
}
macro_rules! single_tag_enum {
    ($name:ident, $tag:literal) => {
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq,
            Serialize,
            Deserialize,
            gproxy_protocol_macros::DeclaredFields,
        )]
        pub enum $name {
            #[serde(rename = $tag)]
            Tag,
        }
    };
}
single_tag_enum!(WebSearchResultType, "web_search_result");
single_tag_enum!(WebSearchErrorType, "web_search_tool_result_error");
single_tag_enum!(WebFetchResultType, "web_fetch_result");
single_tag_enum!(AdvisorErrorType, "advisor_tool_result_error");
single_tag_enum!(AdvisorResultType, "advisor_result");
single_tag_enum!(AdvisorRedactedResultType, "advisor_redacted_result");
single_tag_enum!(CodeExecutionErrorType, "code_execution_tool_result_error");
single_tag_enum!(CodeExecutionOutputType, "code_execution_output");
single_tag_enum!(CodeExecutionResultType, "code_execution_result");
single_tag_enum!(
    EncryptedCodeExecutionResultType,
    "encrypted_code_execution_result"
);
single_tag_enum!(
    BashCodeExecutionErrorType,
    "bash_code_execution_tool_result_error"
);
single_tag_enum!(BashCodeExecutionOutputType, "bash_code_execution_output");
single_tag_enum!(BashCodeExecutionResultType, "bash_code_execution_result");
single_tag_enum!(
    TextEditorErrorType,
    "text_editor_code_execution_tool_result_error"
);
single_tag_enum!(
    TextEditorViewResultType,
    "text_editor_code_execution_view_result"
);
single_tag_enum!(
    TextEditorCreateResultType,
    "text_editor_code_execution_create_result"
);
single_tag_enum!(
    TextEditorReplaceResultType,
    "text_editor_code_execution_str_replace_result"
);
single_tag_enum!(ToolSearchErrorType, "tool_search_tool_result_error");
single_tag_enum!(ToolSearchResultsType, "tool_search_tool_search_result");
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchResult {
    #[serde(rename = "type")]
    pub type_: WebSearchResultType,
    pub encrypted_content: String,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebSearchError {
    #[serde(rename = "type")]
    pub type_: WebSearchErrorType,
    pub error_code: WebSearchErrorCode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebSearchErrorCode {
    InvalidToolInput,
    Unavailable,
    MaxUsesExceeded,
    TooManyRequests,
    QueryTooLong,
    RequestTooLarge,
}
pub type ToolCaller = Caller;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum Caller {
    #[serde(rename = "direct")]
    Direct(DirectCaller),
    #[serde(rename = "code_execution_20250825")]
    Server(ServerCaller),
    #[serde(rename = "code_execution_20260120")]
    Server20260120(ServerCaller20260120),
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct DirectCaller {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ServerCaller {
    pub tool_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
pub struct ServerCaller20260120 {
    pub tool_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpResultContent {
    Text(String),
    Blocks(Vec<TextBlock>),
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchResultContent {
    Error(WebFetchError),
    Result(WebFetchResult),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchError {
    #[serde(rename = "type")]
    pub type_: WebFetchErrorType,
    pub error_code: WebFetchErrorCode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchErrorType {
    WebFetchToolResultError,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum WebFetchErrorCode {
    InvalidToolInput,
    UrlTooLong,
    UrlNotAllowed,
    UrlNotInPriorContext,
    UrlNotAccessible,
    UnsupportedContentType,
    TooManyRequests,
    MaxUsesExceeded,
    Unavailable,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct WebFetchResult {
    #[serde(rename = "type")]
    pub type_: WebFetchResultType,
    pub content: DocumentBlock,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved_at: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AdvisorResultContent {
    Error(AdvisorError),
    Result(AdvisorResult),
    Redacted(AdvisorRedactedResult),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum AdvisorErrorCode {
    MaxUsesExceeded,
    PromptTooLong,
    TooManyRequests,
    Overloaded,
    Unavailable,
    ExecutionTimeExceeded,
    ModelNotFound,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AdvisorError {
    #[serde(rename = "type")]
    pub type_: AdvisorErrorType,
    pub error_code: AdvisorErrorCode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AdvisorResult {
    #[serde(rename = "type")]
    pub type_: AdvisorResultType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct AdvisorRedactedResult {
    #[serde(rename = "type")]
    pub type_: AdvisorRedactedResultType,
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionResultContent {
    Error(CodeExecutionError),
    Result(CodeExecutionResult),
    Encrypted(EncryptedCodeExecutionResult),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum CodeExecutionErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionError {
    #[serde(rename = "type")]
    pub type_: CodeExecutionErrorType,
    pub error_code: CodeExecutionErrorCode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionOutput {
    #[serde(rename = "type")]
    pub type_: CodeExecutionOutputType,
    pub file_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CodeExecutionResult {
    #[serde(rename = "type")]
    pub type_: CodeExecutionResultType,
    pub content: Vec<CodeExecutionOutput>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct EncryptedCodeExecutionResult {
    #[serde(rename = "type")]
    pub type_: EncryptedCodeExecutionResultType,
    pub content: Vec<CodeExecutionOutput>,
    pub encrypted_stdout: String,
    pub return_code: i64,
    pub stderr: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashCodeExecutionResultContent {
    Error(BashCodeExecutionError),
    Result(BashCodeExecutionResult),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum BashCodeExecutionErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    OutputFileTooLarge,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashCodeExecutionError {
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionErrorType,
    pub error_code: BashCodeExecutionErrorCode,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashCodeExecutionOutput {
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionOutputType,
    pub file_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct BashCodeExecutionResult {
    #[serde(rename = "type")]
    pub type_: BashCodeExecutionResultType,
    pub content: Vec<BashCodeExecutionOutput>,
    pub return_code: i64,
    pub stderr: String,
    pub stdout: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorResultContent {
    Error(TextEditorError),
    View(TextEditorViewResult),
    Create(TextEditorCreateResult),
    Replace(TextEditorReplaceResult),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
    FileNotFound,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorError {
    #[serde(rename = "type")]
    pub type_: TextEditorErrorType,
    pub error_code: TextEditorErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextEditorFileType {
    Text,
    Image,
    Pdf,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorViewResult {
    #[serde(rename = "type")]
    pub type_: TextEditorViewResultType,
    pub content: String,
    pub file_type: TextEditorFileType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_lines: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_lines: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorCreateResult {
    #[serde(rename = "type")]
    pub type_: TextEditorCreateResultType,
    pub is_file_update: bool,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextEditorReplaceResult {
    #[serde(rename = "type")]
    pub type_: TextEditorReplaceResultType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lines: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lines: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_lines: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_start: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchResultContent {
    Error(ToolSearchError),
    Search(ToolSearchResults),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolSearchErrorCode {
    InvalidToolInput,
    Unavailable,
    TooManyRequests,
    ExecutionTimeExceeded,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolSearchError {
    #[serde(rename = "type")]
    pub type_: ToolSearchErrorType,
    pub error_code: ToolSearchErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolSearchResults {
    #[serde(rename = "type")]
    pub type_: ToolSearchResultsType,
    pub tool_references: Vec<ToolReferenceBlock>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum MidConversationContent {
    Text(TextBlock),
    Addition(ToolAdditionBlock),
    Removal(ToolRemovalBlock),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolAdditionReference {
    Tool(ToolReference),
    Mcp(McpToolReference),
    Toolset(McpToolsetReference),
    Definition(ToolDefinition),
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolChangeReference {
    Tool(ToolReference),
    Mcp(McpToolReference),
    Toolset(McpToolsetReference),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ToolReference {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ToolReferenceType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolReferenceType {
    ToolReference,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolReference {
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolReferenceType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpToolReferenceType {
    McpToolReference,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpToolsetReference {
    pub server_name: String,
    #[serde(rename = "type")]
    pub type_: McpToolsetReferenceType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpToolsetReferenceType {
    McpToolsetReference,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct FallbackInfo {
    pub model: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum TextBlockType {
    #[serde(rename = "text")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ImageBlockType {
    #[serde(rename = "image")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum DocumentBlockType {
    #[serde(rename = "document")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ThinkingBlockType {
    #[serde(rename = "thinking")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum RedactedThinkingBlockType {
    #[serde(rename = "redacted_thinking")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolUseBlockType {
    #[serde(rename = "tool_use")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolResultBlockType {
    #[serde(rename = "tool_result")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ServerToolUseBlockType {
    #[serde(rename = "server_tool_use")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum SearchResultBlockType {
    #[serde(rename = "search_result")]
    Tag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ToolReferenceBlockType {
    #[serde(rename = "tool_reference")]
    Tag,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SystemMessageClearAt {
    NextUserMessage,
    Never,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SystemMessageOutputConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<super::count_tokens::Effort>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolDefinitionType {
    ToolDefinition,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub type_: ToolDefinitionType,
    pub definition: Box<super::tools::ToolUnion>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum McpToolListingType {
    McpToolListing,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct McpToolListingBlock {
    #[serde(rename = "type")]
    pub type_: McpToolListingType,
    pub mcp_server_name: String,
    pub tools: Vec<super::tools::McpListedTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum BrowserStateType {
    BrowserState,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserStateBlock {
    #[serde(rename = "type")]
    pub type_: BrowserStateType,
    pub tabs: Vec<BrowserStateTab>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_changes: Option<Vec<BrowserStateChange>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserStateTab {
    pub tab_id: String,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserTabOpened {
    pub tab_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserDownloadStarted {
    pub download_id: String,
    pub url: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserDownloadCompleted {
    pub download_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BrowserDownloadFailed {
    pub download_id: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum BrowserStateChange {
    TabOpened(BrowserTabOpened),
    DownloadStarted(BrowserDownloadStarted),
    DownloadCompleted(BrowserDownloadCompleted),
    DownloadFailed(BrowserDownloadFailed),
}
