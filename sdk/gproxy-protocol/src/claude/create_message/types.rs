use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::claude::count_tokens::types as ct;

pub use crate::claude::types::{
    AnthropicBeta, AnthropicBetaKnown, AnthropicVersion, BetaError, BetaErrorResponse,
    BetaErrorResponseType, BetaErrorType, HttpMethod,
};

/// JSON object used for schema/input maps documented as `map[unknown]`.
pub type JsonObject = ct::JsonObject;

/// Model identifier accepted by Claude endpoints.
pub type Model = ct::Model;
pub type ModelKnown = ct::ModelKnown;

/// Reused request-side domain types shared across Claude endpoints.
pub type BetaMessageParam = ct::BetaMessageParam;
pub type BetaContextManagementConfig = ct::BetaContextManagementConfig;
pub type BetaRequestMcpServerUrlDefinition = ct::BetaRequestMcpServerUrlDefinition;
pub type BetaOutputConfig = ct::BetaOutputConfig;
pub type BetaJsonOutputFormat = ct::BetaJsonOutputFormat;
pub type BetaSystemPrompt = ct::BetaSystemPrompt;
pub type BetaCacheControlEphemeral = ct::BetaCacheControlEphemeral;
pub type BetaThinkingConfigParam = ct::BetaThinkingConfigParam;
pub type BetaToolChoice = ct::BetaToolChoice;
pub type BetaToolUnion = ct::BetaToolUnion;

/// Reused response content blocks whose shape matches existing strong types.
pub type BetaThinkingBlock = ct::BetaThinkingBlockParam;
pub type BetaRedactedThinkingBlock = ct::BetaRedactedThinkingBlockParam;
pub type BetaToolUseBlock = ct::BetaToolUseBlockParam;
pub type BetaServerToolUseBlock = ct::BetaServerToolUseBlockParam;
pub type BetaCodeExecutionToolResultBlock = ct::BetaCodeExecutionToolResultBlockParam;
pub type BetaBashCodeExecutionToolResultBlock = ct::BetaBashCodeExecutionToolResultBlockParam;
pub type BetaTextEditorCodeExecutionToolResultBlock =
    ct::BetaTextEditorCodeExecutionToolResultBlockParam;
pub type BetaToolSearchToolResultBlock = ct::BetaToolSearchToolResultBlockParam;
pub type BetaMcpToolUseBlock = ct::BetaMcpToolUseBlockParam;
pub type BetaMcpToolResultBlock = ct::BetaRequestMcpToolResultBlockParam;
pub type BetaContainerUploadBlock = ct::BetaContainerUploadBlockParam;
pub type BetaCompactionBlock = ct::BetaCompactionBlockParam;

pub type BetaWebSearchToolResultErrorCode = ct::BetaWebSearchToolResultErrorCode;
pub type BetaWebFetchToolResultErrorCode = ct::BetaWebFetchToolResultErrorCode;

/// Request `container` can be either a container id string or params object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContainerRef {
    Id(String),
    Params(BetaContainerParams),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct BetaContainerParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<BetaSkillParams>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaSkillParams {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: BetaSkillType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaSkillType {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BetaMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaServiceTierParam {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "standard_only")]
    StandardOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaSpeed {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "fast")]
    Fast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaServiceTier {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "priority")]
    Priority,
    #[serde(rename = "batch")]
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaMessage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainer>,
    pub content: Vec<BetaContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementResponse>,
    pub model: Model,
    pub role: BetaMessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub type_: BetaMessageType,
    pub usage: BetaUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(rename = "compaction")]
    Compaction,
    #[serde(rename = "refusal")]
    Refusal,
    #[serde(rename = "model_context_window_exceeded")]
    ModelContextWindowExceeded,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContainer {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub skills: Vec<BetaSkill>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaSkill {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: BetaSkillType,
    pub version: String,
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
    Compaction(BetaCompactionBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaTextBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<BetaTextCitation>>,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: BetaTextBlockType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaTextBlockType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextCitation {
    CharLocation(BetaCitationCharLocation),
    PageLocation(BetaCitationPageLocation),
    ContentBlockLocation(BetaCitationContentBlockLocation),
    WebSearchResultLocation(BetaCitationsWebSearchResultLocation),
    SearchResultLocation(BetaCitationSearchResultLocation),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationCharLocation {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_char_index: u64,
    pub file_id: String,
    pub start_char_index: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationCharLocationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCitationCharLocationType {
    #[serde(rename = "char_location")]
    CharLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationPageLocation {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_page_number: u64,
    pub file_id: String,
    pub start_page_number: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationPageLocationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCitationPageLocationType {
    #[serde(rename = "page_location")]
    PageLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaCitationContentBlockLocation {
    pub cited_text: String,
    pub document_index: u64,
    pub document_title: String,
    pub end_block_index: u64,
    pub file_id: String,
    pub start_block_index: u64,
    #[serde(rename = "type")]
    pub type_: BetaCitationContentBlockLocationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCitationContentBlockLocationType {
    #[serde(rename = "content_block_location")]
    ContentBlockLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCitationsWebSearchResultLocation {
    pub cited_text: String,
    pub encrypted_index: String,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaCitationsWebSearchResultLocationType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCitationsWebSearchResultLocationType {
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCitationSearchResultLocation {
    pub cited_text: String,
    pub end_block_index: u64,
    pub search_result_index: u64,
    pub source: String,
    pub start_block_index: u64,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaCitationSearchResultLocationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCitationSearchResultLocationType {
    #[serde(rename = "search_result_location")]
    SearchResultLocation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlock {
    pub content: BetaWebSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchToolResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultBlockType {
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultBlockContent {
    Error(BetaWebSearchToolResultError),
    Results(Vec<BetaWebSearchResultBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchToolResultErrorType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebSearchToolResultErrorType {
    #[serde(rename = "web_search_tool_result_error")]
    WebSearchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlock {
    pub encrypted_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaWebSearchResultBlockType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebSearchResultBlockType {
    #[serde(rename = "web_search_result")]
    WebSearchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlock {
    pub content: BetaWebFetchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchToolResultBlockType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockContent {
    Error(BetaWebFetchToolResultErrorBlock),
    Result(BetaWebFetchBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlock {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchToolResultErrorBlockType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebFetchToolResultErrorBlockType {
    #[serde(rename = "web_fetch_tool_result_error")]
    WebFetchToolResultError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaWebFetchBlock {
    pub content: BetaDocumentBlock,
    #[serde(with = "time::serde::rfc3339")]
    pub retrieved_at: OffsetDateTime,
    #[serde(rename = "type")]
    pub type_: BetaWebFetchBlockType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaWebFetchBlockType {
    #[serde(rename = "web_fetch_result")]
    WebFetchResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaDocumentBlock {
    pub citations: BetaCitationConfig,
    pub source: BetaDocumentBlockSource,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: BetaDocumentBlockType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCitationConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaDocumentBlockSource {
    Base64Pdf(ct::BetaBase64PdfSource),
    PlainText(ct::BetaPlainTextSource),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaDocumentBlockType {
    #[serde(rename = "document")]
    Document,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaContextManagementResponse {
    pub applied_edits: Vec<BetaContextManagementAppliedEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementAppliedEdit {
    ClearToolUses(BetaClearToolUses20250919EditResponse),
    ClearThinking(BetaClearThinking20251015EditResponse),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919EditResponse {
    pub cleared_input_tokens: u64,
    pub cleared_tool_uses: u64,
    #[serde(rename = "type")]
    pub type_: BetaClearToolUses20250919EditResponseType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaClearToolUses20250919EditResponseType {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaClearThinking20251015EditResponse {
    pub cleared_input_tokens: u64,
    pub cleared_thinking_turns: u64,
    #[serde(rename = "type")]
    pub type_: BetaClearThinking20251015EditResponseType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaClearThinking20251015EditResponseType {
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaUsage {
    pub cache_creation: BetaCacheCreation,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub inference_geo: String,
    pub input_tokens: u64,
    #[serde(default)]
    pub iterations: BetaIterationsUsage,
    pub output_tokens: u64,
    #[serde(default)]
    pub server_tool_use: BetaServerToolUsage,
    pub service_tier: BetaServiceTier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<BetaSpeed>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCacheCreation {
    pub ephemeral_1h_input_tokens: u64,
    pub ephemeral_5m_input_tokens: u64,
}

pub type BetaIterationsUsage = Vec<BetaIterationUsage>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaIterationUsage {
    Message(BetaMessageIterationUsage),
    Compaction(BetaCompactionIterationUsage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaMessageIterationUsage {
    pub cache_creation: BetaCacheCreation,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(rename = "type")]
    pub type_: BetaMessageIterationUsageType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaMessageIterationUsageType {
    #[serde(rename = "message")]
    Message,
}

/// total_input_tokens = cache_read_input_tokens + cache_creation_input_tokens + input_tokens
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BetaCompactionIterationUsage {
    pub cache_creation: BetaCacheCreation,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(rename = "type")]
    pub type_: BetaCompactionIterationUsageType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BetaCompactionIterationUsageType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BetaServerToolUsage {
    pub web_fetch_requests: u64,
    pub web_search_requests: u64,
}
