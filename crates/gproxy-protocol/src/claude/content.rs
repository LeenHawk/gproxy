use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Rest;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
    WebSearchToolResult(GenericBlock),
    WebFetchToolResult(GenericBlock),
    CodeExecutionToolResult(GenericBlock),
    BashCodeExecutionToolResult(GenericBlock),
    TextEditorCodeExecutionToolResult(GenericBlock),
    ToolSearchToolResult(GenericBlock),
    McpToolUse(GenericBlock),
    McpToolResult(GenericBlock),
    ContainerUpload(GenericBlock),
    Compaction(GenericBlock),
    MidConversationSystem(GenericBlock),
    ToolAddition(GenericBlock),
    ToolRemoval(GenericBlock),
    Fallback(GenericBlock),
}

impl<'de> Deserialize<'de> for ContentBlock {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match value.get("type").and_then(Value::as_str) {
            Some("text") => serde_json::from_value(value)
                .map(Self::Text)
                .map_err(serde::de::Error::custom),
            Some("image") => serde_json::from_value(value)
                .map(Self::Image)
                .map_err(serde::de::Error::custom),
            Some("document") => serde_json::from_value(value)
                .map(Self::Document)
                .map_err(serde::de::Error::custom),
            Some("thinking") => serde_json::from_value(value)
                .map(Self::Thinking)
                .map_err(serde::de::Error::custom),
            Some("redacted_thinking") => serde_json::from_value(value)
                .map(Self::RedactedThinking)
                .map_err(serde::de::Error::custom),
            Some("tool_use") => serde_json::from_value(value)
                .map(Self::ToolUse)
                .map_err(serde::de::Error::custom),
            Some("tool_result") => serde_json::from_value(value)
                .map(Self::ToolResult)
                .map_err(serde::de::Error::custom),
            Some("server_tool_use") => serde_json::from_value(value)
                .map(Self::ServerToolUse)
                .map_err(serde::de::Error::custom),
            Some("search_result") => serde_json::from_value(value)
                .map(Self::SearchResult)
                .map_err(serde::de::Error::custom),
            Some("web_search_tool_result") => serde_json::from_value(value)
                .map(Self::WebSearchToolResult)
                .map_err(serde::de::Error::custom),
            Some("web_fetch_tool_result") => serde_json::from_value(value)
                .map(Self::WebFetchToolResult)
                .map_err(serde::de::Error::custom),
            Some("code_execution_tool_result") => serde_json::from_value(value)
                .map(Self::CodeExecutionToolResult)
                .map_err(serde::de::Error::custom),
            Some("bash_code_execution_tool_result") => serde_json::from_value(value)
                .map(Self::BashCodeExecutionToolResult)
                .map_err(serde::de::Error::custom),
            Some("text_editor_code_execution_tool_result") => serde_json::from_value(value)
                .map(Self::TextEditorCodeExecutionToolResult)
                .map_err(serde::de::Error::custom),
            Some("tool_search_tool_result") => serde_json::from_value(value)
                .map(Self::ToolSearchToolResult)
                .map_err(serde::de::Error::custom),
            Some("mcp_tool_use") => serde_json::from_value(value)
                .map(Self::McpToolUse)
                .map_err(serde::de::Error::custom),
            Some("mcp_tool_result") => serde_json::from_value(value)
                .map(Self::McpToolResult)
                .map_err(serde::de::Error::custom),
            Some("container_upload") => serde_json::from_value(value)
                .map(Self::ContainerUpload)
                .map_err(serde::de::Error::custom),
            Some("compaction") => serde_json::from_value(value)
                .map(Self::Compaction)
                .map_err(serde::de::Error::custom),
            Some("mid_conversation_system") => serde_json::from_value(value)
                .map(Self::MidConversationSystem)
                .map_err(serde::de::Error::custom),
            Some("tool_addition") => serde_json::from_value(value)
                .map(Self::ToolAddition)
                .map_err(serde::de::Error::custom),
            Some("tool_removal") => serde_json::from_value(value)
                .map(Self::ToolRemoval)
                .map_err(serde::de::Error::custom),
            Some("fallback") => serde_json::from_value(value)
                .map(Self::Fallback)
                .map_err(serde::de::Error::custom),
            Some(other) => Err(serde::de::Error::custom(format!(
                "unknown content block type {other}"
            ))),
            None => Err(serde::de::Error::custom("content block requires a type")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum BlockType {
    Text,
    Image,
    Document,
    Thinking,
    RedactedThinking,
    ToolUse,
    ToolResult,
    ServerToolUse,
    SearchResult,
}

/// Temporary storage for server and control blocks whose documented payload
/// schemas have not yet been fully modeled. Raw fields preserve wire data;
/// callers should not treat this as complete typed coverage of those tools.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct GenericBlock {
    // TODO: replace this compatibility envelope with the individually typed
    // service-tool blocks from Count tokens in a Message.md in a later pass.
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
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
pub enum CacheControlType {
    Ephemeral,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct TextBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
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
pub struct ImageBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
    pub source: ImageSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct FileSource {
    pub file_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct DocumentBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
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
pub struct ContentSource {
    pub content: MessageContent,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum PdfMediaType {
    #[serde(rename = "application/pdf")]
    Pdf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum TextMediaType {
    #[serde(rename = "text/plain")]
    Plain,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
pub struct ThinkingBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
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
pub struct RedactedThinkingBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
    pub data: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolUseBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
    pub id: String,
    pub input: Value,
    pub name: String,
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
pub struct ToolResultBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
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
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ServerToolUseBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
    pub id: String,
    pub input: Value,
    pub name: String,
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
pub struct SearchResultBlock {
    #[serde(rename = "type")]
    pub type_: BlockType,
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
