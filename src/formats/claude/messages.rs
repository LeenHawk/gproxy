use serde::de;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use time::OffsetDateTime;

use super::types::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesHeaders {
    #[serde(rename = "anthropic-beta", skip_serializing_if = "Option::is_none")]
    pub anthropic_beta: Option<Vec<AnthropicBeta>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageCreateRequest {
    pub max_tokens: u32,
    pub messages: MessageList,
    pub model: Model,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<BetaRequestMCPServerURLDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BetaMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<BetaOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<BetaJSONOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<UnitIntervalF64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<BetaThinkingConfigParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<BetaToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<BetaToolUnion>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<UnitIntervalF64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMessage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainer>,
    pub content: Vec<BetaContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementResponse>,
    pub model: Model,
    pub role: AssistantRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub message_type: MessageObjectType,
    pub usage: BetaUsage,
}

pub type MessageCreateResponse = BetaMessage;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MessageObjectType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AssistantRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UnitIntervalF64(f64);

impl UnitIntervalF64 {
    pub const MIN: f64 = 0.0;
    pub const MAX: f64 = 1.0;

    pub fn new(value: f64) -> Result<Self, String> {
        if value.is_finite() && (Self::MIN..=Self::MAX).contains(&value) {
            Ok(Self(value))
        } else {
            Err(format!(
                "value must be in [{}, {}] (got {})",
                Self::MIN,
                Self::MAX,
                value
            ))
        }
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

impl Serialize for UnitIntervalF64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

impl<'de> Deserialize<'de> for UnitIntervalF64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        UnitIntervalF64::new(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContainerRef {
    Params(BetaContainerParams),
    Id(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainerParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<BetaSkillParams>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSkillParams {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub skill_type: SkillType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceTier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "standard_only")]
    StandardOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillType {
    #[serde(rename = "anthropic")]
    Anthropic,
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    MCPToolUse(BetaMCPToolUseBlock),
    MCPToolResult(BetaMCPToolResultBlock),
    ContainerUpload(BetaContainerUploadBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextBlock {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<BetaTextCitation>,
    pub text: String,
    #[serde(rename = "type")]
    pub block_type: TextBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BetaTextCitation {
    #[serde(rename = "char_location")]
    CharLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_char_index: u32,
        file_id: String,
        start_char_index: u32,
    },
    #[serde(rename = "page_location")]
    PageLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_page_number: u32,
        file_id: String,
        start_page_number: u32,
    },
    #[serde(rename = "content_block_location")]
    ContentBlockLocation {
        cited_text: String,
        document_index: u32,
        document_title: String,
        end_block_index: u32,
        file_id: String,
        start_block_index: u32,
    },
    #[serde(rename = "web_search_result_location")]
    WebSearchResultLocation {
        cited_text: String,
        encrypted_index: String,
        title: String,
        url: String,
    },
    #[serde(rename = "search_result_location")]
    SearchResultLocation {
        cited_text: String,
        end_block_index: u32,
        search_result_index: u32,
        source: String,
        start_block_index: u32,
        title: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaThinkingBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub thinking: String,
    #[serde(rename = "type")]
    pub block_type: ThinkingBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaRedactedThinkingBlock {
    pub data: String,
    #[serde(rename = "type")]
    pub block_type: RedactedThinkingBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolUseBlock {
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: String,
    #[serde(rename = "type")]
    pub block_type: ToolUseBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaCaller>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaServerToolUseBlock {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaCaller>,
    pub input: HashMap<String, Value>,
    pub name: ServerToolName,
    #[serde(rename = "type")]
    pub block_type: ServerToolUseBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultBlock {
    pub content: BetaWebSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: WebSearchToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebSearchToolResultBlockContent {
    Error(BetaWebSearchToolResultError),
    Results(Vec<BetaWebSearchResultBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchToolResultError {
    pub error_code: BetaWebSearchToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: WebSearchToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebSearchResultBlock {
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_age: Option<String>,
    pub title: String,
    #[serde(rename = "type")]
    pub block_type: WebSearchResultBlockType,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultBlock {
    pub content: BetaWebFetchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: WebFetchToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaWebFetchToolResultBlockContent {
    Error(BetaWebFetchToolResultErrorBlock),
    Result(BetaWebFetchBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchToolResultErrorBlock {
    pub error_code: BetaWebFetchToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: WebFetchToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaWebFetchBlock {
    pub content: BetaDocumentBlock,
    #[serde(with = "time::serde::rfc3339")]
    pub retrieved_at: OffsetDateTime,
    #[serde(rename = "type")]
    pub block_type: WebFetchResultType,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaDocumentBlock {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<BetaCitationConfig>,
    pub source: BetaDocumentBlockSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub block_type: DocumentBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaDocumentBlockSource {
    Base64(BetaBase64PDFSource),
    PlainText(BetaPlainTextSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCitationConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultBlock {
    pub content: BetaCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: CodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaCodeExecutionToolResultBlockContent {
    Error(BetaCodeExecutionToolResultError),
    Result(BetaCodeExecutionResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionToolResultError {
    pub error_code: BetaCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: CodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionResultBlock {
    pub content: Vec<BetaCodeExecutionOutputBlock>,
    pub return_code: u32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub block_type: CodeExecutionResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub output_type: CodeExecutionOutputType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultBlock {
    pub content: BetaBashCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: BashCodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaBashCodeExecutionToolResultBlockContent {
    Error(BetaBashCodeExecutionToolResultError),
    Result(BetaBashCodeExecutionResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionToolResultError {
    pub error_code: BetaBashCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: BashCodeExecutionToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionResultBlock {
    pub content: Vec<BetaBashCodeExecutionOutputBlock>,
    pub return_code: u32,
    pub stderr: String,
    pub stdout: String,
    #[serde(rename = "type")]
    pub block_type: BashCodeExecutionResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaBashCodeExecutionOutputBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub output_type: BashCodeExecutionOutputType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultBlock {
    pub content: BetaTextEditorCodeExecutionToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaTextEditorCodeExecutionToolResultBlockContent {
    Error(BetaTextEditorCodeExecutionToolResultError),
    View(BetaTextEditorCodeExecutionViewResultBlock),
    Create(BetaTextEditorCodeExecutionCreateResultBlock),
    StrReplace(BetaTextEditorCodeExecutionStrReplaceResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionToolResultError {
    pub error_code: BetaTextEditorCodeExecutionToolResultErrorCode,
    #[serde(rename = "type")]
    pub error_type: TextEditorCodeExecutionToolResultErrorType,
    pub error_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionViewResultBlock {
    pub content: String,
    pub file_type: TextEditorCodeExecutionFileType,
    pub num_lines: u32,
    pub start_line: u32,
    pub total_lines: u32,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionViewResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionCreateResultBlock {
    pub is_file_update: bool,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionCreateResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaTextEditorCodeExecutionStrReplaceResultBlock {
    pub lines: Vec<String>,
    pub new_lines: u32,
    pub new_start: u32,
    pub old_lines: u32,
    pub old_start: u32,
    #[serde(rename = "type")]
    pub block_type: TextEditorCodeExecutionStrReplaceResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultBlock {
    pub content: BetaToolSearchToolResultBlockContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: ToolSearchToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaToolSearchToolResultBlockContent {
    Error(BetaToolSearchToolResultError),
    Result(BetaToolSearchToolSearchResultBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolResultError {
    pub error_code: BetaToolSearchToolResultErrorCode,
    pub error_message: String,
    #[serde(rename = "type")]
    pub error_type: ToolSearchToolResultErrorType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaToolSearchToolSearchResultBlock {
    pub tool_references: Vec<BetaToolReferenceBlockParam>,
    #[serde(rename = "type")]
    pub block_type: ToolSearchToolSearchResultType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolUseBlock {
    pub id: String,
    pub input: HashMap<String, Value>,
    pub name: String,
    pub server_name: String,
    #[serde(rename = "type")]
    pub block_type: MCPToolUseBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaMCPToolResultBlock {
    pub content: MCPToolResultContent,
    pub is_error: bool,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub block_type: MCPToolResultBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPToolResultContent {
    Text(String),
    Blocks(Vec<BetaTextBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainerUploadBlock {
    pub file_id: String,
    #[serde(rename = "type")]
    pub block_type: ContainerUploadBlockType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContextManagementResponse {
    pub applied_edits: Vec<BetaContextManagementEditResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaContextManagementEditResponse {
    ClearToolUses(BetaClearToolUses20250919EditResponse),
    ClearThinking(BetaClearThinking20251015EditResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearToolUses20250919EditResponse {
    pub cleared_input_tokens: u32,
    pub cleared_tool_uses: u32,
    #[serde(rename = "type")]
    pub response_type: ClearToolUsesEditResponseType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ClearToolUsesEditResponseType {
    #[serde(rename = "clear_tool_uses_20250919")]
    ClearToolUses20250919,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaClearThinking20251015EditResponse {
    pub cleared_input_tokens: u32,
    pub cleared_thinking_turns: u32,
    #[serde(rename = "type")]
    pub response_type: ClearThinkingEditResponseType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ClearThinkingEditResponseType {
    #[serde(rename = "clear_thinking_20251015")]
    ClearThinking20251015,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaContainer {
    pub id: String,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub skills: Vec<BetaSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaSkill {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub skill_type: SkillType,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<BetaCacheCreation>,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<BetaServerToolUsage>,
    pub service_tier: UsageServiceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaCacheCreation {
    pub ephemeral_1h_input_tokens: u32,
    pub ephemeral_5m_input_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BetaServerToolUsage {
    pub web_fetch_requests: u32,
    pub web_search_requests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsageServiceTier {
    #[serde(rename = "standard")]
    Standard,
    #[serde(rename = "priority")]
    Priority,
    #[serde(rename = "batch")]
    Batch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl fmt::Display for UnitIntervalF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
