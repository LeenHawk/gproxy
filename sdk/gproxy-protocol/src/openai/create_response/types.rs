use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use crate::openai::count_tokens::types::{
    JsonObject, ResponseApplyPatchCall, ResponseApplyPatchCallOutput,
    ResponseCodeInterpreterToolCall, ResponseCompactionItemParam, ResponseComputerCallOutput,
    ResponseComputerToolCall, ResponseConversation, ResponseConversationParam,
    ResponseCustomToolCall, ResponseCustomToolCallOutput, ResponseFileSearchToolCall,
    ResponseFunctionCallOutput, ResponseFunctionToolCall, ResponseFunctionWebSearch,
    ResponseImageGenerationCallStatus, ResponseImageGenerationCallType, ResponseInput,
    ResponseInputFile, ResponseInputImage, ResponseInputItem, ResponseInputText,
    ResponseItemReference, ResponseLocalShellCall, ResponseLocalShellCallOutput,
    ResponseMcpApprovalRequest, ResponseMcpApprovalResponse, ResponseMcpCall, ResponseMcpListTools,
    ResponseMessagePhase, ResponseOutputMessage, ResponseOutputRefusal, ResponseOutputText,
    ResponseReasoning, ResponseReasoningItem, ResponseReasoningTextContent, ResponseShellCall,
    ResponseShellCallOutput, ResponseSummaryTextContent, ResponseTextConfig, ResponseTool,
    ResponseToolChoice, ResponseToolSearchCall, ResponseToolSearchOutput, ResponseTruncation,
};
pub use crate::openai::types::{
    HttpMethod, OpenAiApiError, OpenAiApiErrorResponse, OpenAiResponseHeaders,
};

/// Metadata map (string key-value pairs).
pub type Metadata = BTreeMap<String, String>;

/// Model identifier.
pub type Model = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseContextManagementEntry {
    #[serde(rename = "type")]
    pub type_: ResponseContextManagementType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compact_threshold: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContextManagementType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseIncludable {
    #[serde(rename = "file_search_call.results")]
    FileSearchCallResults,
    #[serde(rename = "web_search_call.results")]
    WebSearchCallResults,
    #[serde(rename = "web_search_call.action.sources")]
    WebSearchCallActionSources,
    #[serde(rename = "message.input_image.image_url")]
    MessageInputImageImageUrl,
    #[serde(rename = "computer_call_output.output.image_url")]
    ComputerCallOutputOutputImageUrl,
    #[serde(rename = "code_interpreter_call.outputs")]
    CodeInterpreterCallOutputs,
    #[serde(rename = "reasoning.encrypted_content")]
    ReasoningEncryptedContent,
    #[serde(rename = "message.output_text.logprobs")]
    MessageOutputTextLogprobs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponsePrompt {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<BTreeMap<String, ResponsePromptVariable>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponsePromptVariable {
    String(String),
    InputText(ResponseInputText),
    InputImage(ResponseInputImage),
    InputFile(ResponseInputFile),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponsePromptCacheRetention {
    #[serde(rename = "in-memory")]
    InMemory,
    #[serde(rename = "24h")]
    H24,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseServiceTier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default")]
    Default,
    #[serde(rename = "flex")]
    Flex,
    #[serde(rename = "scale")]
    Scale,
    #[serde(rename = "priority")]
    Priority,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: ResponseErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseErrorCode {
    #[serde(rename = "server_error")]
    ServerError,
    #[serde(rename = "rate_limit_exceeded")]
    RateLimitExceeded,
    #[serde(rename = "invalid_prompt")]
    InvalidPrompt,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseIncompleteDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ResponseIncompleteReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseIncompleteReason {
    #[serde(rename = "max_output_tokens")]
    MaxOutputTokens,
    #[serde(rename = "content_filter")]
    ContentFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseObject {
    #[serde(rename = "response")]
    Response,
}

/// Output item union returned by the Responses API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutputItem {
    Message(ResponseOutputMessage),
    FileSearchToolCall(ResponseFileSearchToolCall),
    ComputerToolCall(ResponseComputerToolCall),
    ComputerCallOutput(ResponseComputerCallOutput),
    ToolSearchCall(ResponseToolSearchCall),
    ToolSearchOutput(ResponseToolSearchOutput),
    FunctionWebSearch(ResponseFunctionWebSearch),
    FunctionToolCall(ResponseFunctionToolCall),
    FunctionCallOutput(ResponseFunctionCallOutput),
    ReasoningItem(ResponseReasoningItem),
    CompactionItem(ResponseCompactionItemParam),
    ImageGenerationCall(ResponseOutputImageGenerationCall),
    CodeInterpreterToolCall(ResponseCodeInterpreterToolCall),
    LocalShellCall(ResponseLocalShellCall),
    LocalShellCallOutput(ResponseLocalShellCallOutput),
    ShellCall(ResponseShellCall),
    ShellCallOutput(ResponseShellCallOutput),
    ApplyPatchCall(ResponseApplyPatchCall),
    ApplyPatchCallOutput(ResponseApplyPatchCallOutput),
    McpListTools(ResponseMcpListTools),
    McpApprovalRequest(ResponseMcpApprovalRequest),
    McpApprovalResponse(ResponseMcpApprovalResponse),
    McpCall(ResponseMcpCall),
    CustomToolCallOutput(ResponseCustomToolCallOutput),
    CustomToolCall(ResponseCustomToolCall),
    ItemReference(ResponseItemReference),
}

/// Image generation call as it appears inside a Responses API `output` array
/// and inside `response.output_item.added` / `response.output_item.done`
/// streaming events.
///
/// This is intentionally looser than `count_tokens::ResponseImageGenerationCall`
/// (the "final, completed item" shape used as input to count-tokens). The
/// streaming lifecycle emits the item on `output_item.added` before the image
/// exists — `result` is only populated when the generation completes, and the
/// Codex backend additionally ships runtime metadata (`action`, `background`,
/// `output_format`, `quality`, `size`, `revised_prompt`) that isn't in the
/// OpenAI public spec. Keeping the input schema strict while the output schema
/// tolerates partial snapshots avoids breaking `ResponseOutputItem`
/// deserialization on `output_item.added` without loosening the count-tokens
/// contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputImageGenerationCall {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub status: ResponseImageGenerationCallStatus,
    #[serde(rename = "type")]
    pub type_: ResponseImageGenerationCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ResponseStreamOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: u64,
    pub input_tokens_details: ResponseInputTokensDetails,
    pub output_tokens: u64,
    pub output_tokens_details: ResponseOutputTokensDetails,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseInputTokensDetails {
    pub cached_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseOutputTokensDetails {
    pub reasoning_tokens: u64,
}
