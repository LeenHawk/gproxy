//! OpenAI Responses response, Create a model response.md:5768-14778.
//! Nullable usage/user also follow the native created/in_progress examples at
//! Create a model response.md:15512-15515 and Responses.md:33257-33260.
use super::generate;
use super::input::{self, present_nullable, present_optional, required_nullable};
use super::tools;
use crate::Rest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type GenerateContentResponse = crate::WireResponse<GenerateContentResponseBody>;
/// Returned instructions have the complete native input union, including history.
pub type ResponseInstructions = input::Input;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct GenerateContentResponseBody {
    pub id: String,
    pub created_at: i64,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub error: Option<ResponseError>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub incomplete_details: Option<ResponseIncompleteDetails>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub instructions: Option<ResponseInstructions>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub metadata: Option<BTreeMap<String, String>>,
    pub model: String,
    pub object: ResponseObject,
    pub output: Vec<ResponseOutputItem>,
    pub parallel_tool_calls: bool,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub temperature: Option<serde_json::Number>,
    pub tool_choice: input::ToolChoice,
    pub tools: Vec<tools::Tool>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub top_p: Option<serde_json::Number>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub background: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub completed_at: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub conversation: Option<Option<input::ResponseConversationParam>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_tool_calls: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub moderation: Option<Option<ResponseModerationReport>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_text: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_response_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt: Option<Option<generate::ResponsePrompt>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_options: Option<ResponsePromptCacheOptions>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_retention: Option<Option<generate::PromptCacheRetention>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<Option<input::ReasoningConfig>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub safety_identifier: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_tier: Option<Option<generate::ServiceTier>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<ResponseStatus>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<input::TextConfig>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub top_logprobs: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub truncation: Option<Option<generate::GenerateTruncation>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub usage: Option<Option<ResponseUsage>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub user: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseObject {
    #[serde(rename = "response")]
    Response,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseError {
    pub code: ResponseErrorCode,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseErrorCode {
    #[serde(rename = "server_error")]
    ServerError,
    #[serde(rename = "rate_limit_exceeded")]
    RateLimitExceeded,
    #[serde(rename = "invalid_prompt")]
    InvalidPrompt,
    #[serde(rename = "data_residency_mismatch")]
    DataResidencyMismatch,
    #[serde(rename = "bio_policy")]
    BioPolicy,
    #[serde(rename = "vector_store_timeout")]
    VectorStoreTimeout,
    #[serde(rename = "invalid_image")]
    InvalidImage,
    #[serde(rename = "invalid_image_format")]
    InvalidImageFormat,
    #[serde(rename = "invalid_base64_image")]
    InvalidBase64Image,
    #[serde(rename = "invalid_image_url")]
    InvalidImageUrl,
    #[serde(rename = "image_too_large")]
    ImageTooLarge,
    #[serde(rename = "image_too_small")]
    ImageTooSmall,
    #[serde(rename = "image_parse_error")]
    ImageParseError,
    #[serde(rename = "image_content_policy_violation")]
    ImageContentPolicyViolation,
    #[serde(rename = "invalid_image_mode")]
    InvalidImageMode,
    #[serde(rename = "image_file_too_large")]
    ImageFileTooLarge,
    #[serde(rename = "unsupported_image_media_type")]
    UnsupportedImageMediaType,
    #[serde(rename = "empty_image_file")]
    EmptyImageFile,
    #[serde(rename = "failed_to_download_image")]
    FailedToDownloadImage,
    #[serde(rename = "image_file_not_found")]
    ImageFileNotFound,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseIncompleteDetails {
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub reason: Option<ResponseIncompleteReason>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseIncompleteReason {
    #[serde(rename = "max_output_tokens")]
    MaxOutputTokens,
    #[serde(rename = "content_filter")]
    ContentFilter,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponsePromptCacheOptions {
    pub mode: generate::PromptCachingMode,
    pub ttl: generate::PromptCacheTtl,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseModerationReport {
    pub input: ResponseModerationOutcome,
    pub output: ResponseModerationOutcome,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseModerationOutcome {
    Result(ResponseModerationResult),
    Error(ResponseModerationError),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseModerationResult {
    pub categories: BTreeMap<String, bool>,
    pub category_applied_input_types: BTreeMap<String, Vec<ResponseModerationInputType>>,
    pub category_scores: BTreeMap<String, serde_json::Number>,
    pub flagged: bool,
    pub model: String,
    #[serde(rename = "type")]
    pub type_: ResponseModerationResultType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseModerationInputType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseModerationResultType {
    #[serde(rename = "moderation_result")]
    ModerationResult,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseModerationError {
    pub code: String,
    pub message: String,
    #[serde(rename = "type")]
    pub type_: ResponseModerationErrorType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseModerationErrorType {
    #[serde(rename = "error")]
    Error,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseUsage {
    pub input_tokens: i64,
    pub input_tokens_details: ResponseInputTokensDetails,
    pub output_tokens: i64,
    pub output_tokens_details: ResponseOutputTokensDetails,
    pub total_tokens: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseInputTokensDetails {
    pub cache_write_tokens: i64,
    pub cached_tokens: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputTokensDetails {
    pub reasoning_tokens: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Exactly the 28 output variants; input-only messages/references/triggers are excluded.
/// Same-shape items are reused; response-specific required fields use separate structs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseOutputItem {
    Message(input::ResponseOutputMessage),
    FileSearchCall(input::FileSearchCall),
    FunctionCall(input::FunctionCall),
    FunctionCallOutput(ResponseFunctionCallOutput),
    WebSearchCall(input::WebSearchCall),
    ComputerCall(input::ComputerCall),
    ComputerCallOutput(ResponseComputerCallOutput),
    Reasoning(input::ReasoningItem),
    Program(input::Program),
    ProgramOutput(input::ProgramOutput),
    ToolSearchCall(ResponseToolSearchCall),
    ToolSearchOutput(ResponseToolSearchOutput),
    AdditionalTools(ResponseAdditionalTools),
    Compaction(ResponseCompaction),
    ImageGenerationCall(input::ImageGenerationCall),
    CodeInterpreterCall(input::CodeInterpreterCall),
    LocalShellCall(input::LocalShellCall),
    LocalShellCallOutput(input::LocalShellCallOutput),
    ShellCall(ResponseShellCall),
    ShellCallOutput(ResponseShellCallOutput),
    ApplyPatchCall(ResponseApplyPatchCall),
    ApplyPatchCallOutput(ResponseApplyPatchCallOutput),
    McpCall(input::McpCall),
    McpListTools(input::McpListTools),
    McpApprovalRequest(input::McpApprovalRequest),
    McpApprovalResponse(ResponseMcpApprovalResponse),
    CustomToolCall(input::CustomToolCall),
    CustomToolCallOutput(ResponseCustomToolCallOutput),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseFunctionCallOutput {
    pub id: String,
    pub call_id: String,
    pub output: input::CustomOutput,
    pub status: input::ItemStatus,
    #[serde(rename = "type")]
    pub type_: input::FunctionCallOutputType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub namespace: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseComputerCallOutput {
    pub id: String,
    pub call_id: String,
    pub output: input::ComputerScreenshot,
    pub status: ResponseComputerOutputStatus,
    #[serde(rename = "type")]
    pub type_: input::ComputerCallOutputType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub acknowledged_safety_checks: Option<Vec<input::PendingSafetyCheck>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseComputerOutputStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "in_progress")]
    InProgress,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseToolSearchCall {
    pub id: String,
    // Source line 10675 explicitly permits unknown (arbitrary JSON) arguments.
    pub arguments: serde_json::Value,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub call_id: Option<String>,
    pub execution: tools::ToolExecution,
    pub status: input::ItemStatus,
    #[serde(rename = "type")]
    pub type_: input::ToolSearchCallType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseToolSearchOutput {
    pub id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub call_id: Option<String>,
    pub execution: tools::ToolExecution,
    pub status: input::ItemStatus,
    pub tools: Vec<tools::Tool>,
    #[serde(rename = "type")]
    pub type_: input::ToolSearchOutputType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseAdditionalTools {
    pub id: String,
    pub role: ResponseAdditionalToolsRole,
    pub tools: Vec<tools::Tool>,
    #[serde(rename = "type")]
    pub type_: input::AdditionalToolsType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseAdditionalToolsRole {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "critic")]
    Critic,
    #[serde(rename = "discriminator")]
    Discriminator,
    #[serde(rename = "developer")]
    Developer,
    #[serde(rename = "tool")]
    Tool,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseCompaction {
    pub id: String,
    pub encrypted_content: String,
    #[serde(rename = "type")]
    pub type_: input::CompactionType,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseShellCall {
    pub id: String,
    pub action: ResponseShellAction,
    pub call_id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub environment: Option<ResponseShellEnvironment>,
    pub status: input::ItemStatus,
    #[serde(rename = "type")]
    pub type_: input::ShellCallType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseShellAction {
    pub commands: Vec<String>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub max_output_length: Option<i64>,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub timeout_ms: Option<i64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseShellEnvironment {
    Local(ResponseLocalEnvironment),
    Reference(tools::ShellContainerReference),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseLocalEnvironment {
    #[serde(rename = "type")]
    pub type_: tools::ShellLocalType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseShellCallOutput {
    pub id: String,
    pub call_id: String,
    #[wire(required)]
    #[serde(deserialize_with = "required_nullable")]
    pub max_output_length: Option<i64>,
    pub output: Vec<ResponseShellOutputContent>,
    pub status: input::ItemStatus,
    #[serde(rename = "type")]
    pub type_: input::ShellCallOutputType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseShellOutputContent {
    pub outcome: input::ShellOutputOutcome,
    pub stderr: String,
    pub stdout: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseApplyPatchCall {
    pub id: String,
    pub call_id: String,
    pub operation: input::ApplyPatchOperation,
    pub status: input::ApplyPatchStatus,
    #[serde(rename = "type")]
    pub type_: input::ApplyPatchCallType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseApplyPatchCallOutput {
    pub id: String,
    pub call_id: String,
    pub status: input::ApplyPatchOutputStatus,
    #[serde(rename = "type")]
    pub type_: input::ApplyPatchCallOutputType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpApprovalResponse {
    pub id: String,
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(rename = "type")]
    pub type_: input::McpApprovalResponseType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reason: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseCustomToolCallOutput {
    pub id: String,
    pub call_id: String,
    pub output: input::CustomOutput,
    pub status: input::ItemStatus,
    #[serde(rename = "type")]
    pub type_: input::CustomToolCallOutputType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller: Option<Option<input::Caller>>,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_by: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
