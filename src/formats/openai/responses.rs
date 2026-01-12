use serde::{Deserialize, Serialize};
use serde_valid::Validate;
use serde_valid::validation::Error as ValidationError;

use super::types::{
    Annotation, ApplyPatchToolCall, ApplyPatchToolCallOutput, CodeInterpreterToolCall,
    CompactionBody, ComputerCallSafetyCheckParam, ComputerScreenshotImage, ComputerToolCall,
    ConversationParam, ConversationParamObject, CustomToolCall, FileSearchToolCall,
    FunctionShellCall, FunctionShellCallOutput, FunctionToolCall, ImageGenToolCall, InputItem,
    InputMessageContentList, InputMessageRole, InputMessageType, InputParam, LocalShellToolCall,
    LocalShellToolCallOutput, MCPApprovalRequest, MCPListTools, MCPToolCall, MessageStatus,
    OutputContent, OutputMessage, Prompt, PromptCacheRetention, Reasoning, ReasoningEffort,
    ReasoningItem, ResponseStreamOptions, ResponseTextParam, ServiceTier, SummaryType, Tool,
    ToolChoiceParam, ToolCallOutput, Truncation, WebSearchToolCall,
};

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(rename_all = "snake_case")]
#[validate(custom = validate_responses_request)]
pub struct CreateResponseRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<IncludeEnum>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<super::types::Metadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<ResponseStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 2.0)]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub text: Option<ResponseTextParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoiceParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0)]
    #[validate(maximum = 20)]
    pub top_logprobs: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate(minimum = 0.0)]
    #[validate(maximum = 1.0)]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[validate(custom = validate_compact_request)]
pub struct CompactResponseRequest {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[validate]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

fn validate_responses_request(req: &CreateResponseRequest) -> Result<(), ValidationError> {
    if req.conversation.is_some() && req.previous_response_id.is_some() {
        return Err(ValidationError::Custom(
            "conversation and previous_response_id are mutually exclusive".to_string(),
        ));
    }

    if req.stream_options.is_some() && req.stream != Some(true) {
        return Err(ValidationError::Custom(
            "stream_options requires stream=true".to_string(),
        ));
    }

    if let Some(metadata) = &req.metadata {
        if metadata.len() > 16 {
            return Err(ValidationError::Custom(
                "metadata must not contain more than 16 entries".to_string(),
            ));
        }
        for (key, value) in metadata {
            if key.len() > 64 {
                return Err(ValidationError::Custom(
                    "metadata keys must be at most 64 characters".to_string(),
                ));
            }
            if value.len() > 512 {
                return Err(ValidationError::Custom(
                    "metadata values must be at most 512 characters".to_string(),
                ));
            }
        }
    }

    Ok(())
}

fn validate_compact_request(req: &CompactResponseRequest) -> Result<(), ValidationError> {
    if req.model.trim().is_empty() {
        return Err(ValidationError::Custom(
            "model must not be empty".to_string(),
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IncludeEnum {
    #[serde(rename = "file_search_call.results")]
    FileSearchCallResults,
    #[serde(rename = "web_search_call.results")]
    WebSearchCallResults,
    #[serde(rename = "web_search_call.action.sources")]
    WebSearchCallActionSources,
    #[serde(rename = "message.input_image.image_url")]
    MessageInputImageUrl,
    #[serde(rename = "computer_call_output.output.image_url")]
    ComputerCallOutputImageUrl,
    #[serde(rename = "code_interpreter_call.outputs")]
    CodeInterpreterCallOutputs,
    #[serde(rename = "reasoning.encrypted_content")]
    ReasoningEncryptedContent,
    #[serde(rename = "message.output_text.logprobs")]
    MessageOutputTextLogprobs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInstructions {
    Text(String),
    Items(Vec<InputItem>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseObject {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: ResponseObjectType,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseStatus>,
    pub error: Option<ResponseError>,
    pub incomplete_details: Option<ResponseIncompleteDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    pub instructions: Option<ResponseInstructions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<InputParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<i64>,
    pub model: String,
    pub output: Vec<ResponseOutputItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_retention: Option<PromptCacheRetention>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponseTextParam>,
    pub tool_choice: ToolChoiceParam,
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<i64>,
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ResponseUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub metadata: super::types::Metadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation: Option<ConversationParamObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseObjectType {
    #[serde(rename = "response")]
    Response,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseDeletedResource {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: ResponseObjectType,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseItemList {
    #[serde(rename = "object")]
    pub object_type: ResponseListObjectType,
    pub data: Vec<ItemResource>,
    pub has_more: bool,
    pub first_id: String,
    pub last_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseListObjectType {
    #[serde(rename = "list")]
    List,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ItemResource {
    InputMessage(InputMessageResource),
    OutputMessage(OutputMessage),
    FileSearchToolCall(FileSearchToolCall),
    ComputerToolCall(ComputerToolCall),
    ComputerToolCallOutput(ComputerToolCallOutputResource),
    WebSearchToolCall(WebSearchToolCall),
    FunctionToolCall(FunctionToolCallResource),
    FunctionToolCallOutput(FunctionToolCallOutputResource),
    ImageGenToolCall(ImageGenToolCall),
    CodeInterpreterToolCall(CodeInterpreterToolCall),
    LocalShellToolCall(LocalShellToolCall),
    LocalShellToolCallOutput(LocalShellToolCallOutput),
    FunctionShellCall(FunctionShellCall),
    FunctionShellCallOutput(FunctionShellCallOutput),
    ApplyPatchToolCall(ApplyPatchToolCall),
    ApplyPatchToolCallOutput(ApplyPatchToolCallOutput),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    MCPApprovalResponse(MCPApprovalResponseResource),
    MCPToolCall(MCPToolCall),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputMessageResource {
    #[serde(rename = "type")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_type: Option<InputMessageType>,
    pub id: String,
    pub role: InputMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
    pub content: InputMessageContentList,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionToolCallResource {
    #[serde(rename = "type")]
    pub call_type: super::types::FunctionToolCallType,
    pub id: String,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionToolCallOutputResource {
    #[serde(rename = "type")]
    pub output_type: super::types::FunctionToolCallOutputType,
    pub id: String,
    pub call_id: String,
    pub output: ToolCallOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputerToolCallOutputResource {
    #[serde(rename = "type")]
    pub output_type: super::types::ComputerToolCallOutputType,
    pub id: String,
    pub call_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<ComputerCallSafetyCheckParam>>,
    pub output: ComputerScreenshotImage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPApprovalResponseResource {
    #[serde(rename = "type")]
    pub item_type: super::types::MCPApprovalResponseType,
    pub id: String,
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactResponseResource {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: CompactResponseObjectType,
    pub output: Vec<OutputItem>,
    pub created_at: i64,
    pub usage: ResponseUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompactResponseObjectType {
    #[serde(rename = "response.compaction")]
    ResponseCompaction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputItem {
    InputMessage(InputMessageResource),
    Message(OutputMessage),
    FileSearchToolCall(FileSearchToolCall),
    FunctionToolCall(FunctionToolCall),
    WebSearchToolCall(WebSearchToolCall),
    ComputerToolCall(ComputerToolCall),
    ReasoningItem(ReasoningItem),
    Compaction(CompactionBody),
    ImageGenToolCall(ImageGenToolCall),
    CodeInterpreterToolCall(CodeInterpreterToolCall),
    LocalShellToolCall(LocalShellToolCall),
    FunctionShellCall(FunctionShellCall),
    FunctionShellCallOutput(FunctionShellCallOutput),
    ApplyPatchToolCall(ApplyPatchToolCall),
    ApplyPatchToolCallOutput(ApplyPatchToolCallOutput),
    MCPToolCall(MCPToolCall),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    CustomToolCall(CustomToolCall),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: ResponseErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseIncompleteDetails {
    pub reason: ResponseIncompleteReason,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseIncompleteReason {
    #[serde(rename = "max_output_tokens")]
    MaxOutputTokens,
    #[serde(rename = "content_filter")]
    ContentFilter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutputItem {
    Message(OutputMessage),
    FileSearchToolCall(FileSearchToolCall),
    FunctionToolCall(FunctionToolCall),
    WebSearchToolCall(WebSearchToolCall),
    ComputerToolCall(super::types::ComputerToolCall),
    ReasoningItem(ReasoningItem),
    Compaction(CompactionBody),
    ImageGenToolCall(ImageGenToolCall),
    CodeInterpreterToolCall(CodeInterpreterToolCall),
    LocalShellToolCall(LocalShellToolCall),
    FunctionShellCall(FunctionShellCall),
    FunctionShellCallOutput(FunctionShellCallOutput),
    ApplyPatchToolCall(ApplyPatchToolCall),
    ApplyPatchToolCallOutput(ApplyPatchToolCallOutput),
    MCPToolCall(MCPToolCall),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    CustomToolCall(CustomToolCall),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseUsage {
    pub input_tokens: i64,
    pub input_tokens_details: ResponseUsageInputTokensDetails,
    pub output_tokens: i64,
    pub output_tokens_details: ResponseUsageOutputTokensDetails,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseUsageInputTokensDetails {
    pub cached_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseUsageOutputTokensDetails {
    pub reasoning_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseTopLogProb {
    pub token: String,
    pub logprob: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseLogProb {
    pub token: String,
    pub logprob: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<ResponseTopLogProb>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryTextContent {
    #[serde(rename = "type")]
    pub content_type: SummaryType,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseStreamEvent {
    #[serde(rename = "response.audio.delta")]
    ResponseAudioDelta {
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.audio.done")]
    ResponseAudioDone { sequence_number: i64 },
    #[serde(rename = "response.audio.transcript.delta")]
    ResponseAudioTranscriptDelta {
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.audio.transcript.done")]
    ResponseAudioTranscriptDone { sequence_number: i64 },
    #[serde(rename = "response.code_interpreter_call_code.delta")]
    ResponseCodeInterpreterCallCodeDelta {
        output_index: i64,
        item_id: String,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.code_interpreter_call_code.done")]
    ResponseCodeInterpreterCallCodeDone {
        output_index: i64,
        item_id: String,
        code: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.code_interpreter_call.completed")]
    ResponseCodeInterpreterCallCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.code_interpreter_call.in_progress")]
    ResponseCodeInterpreterCallInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.code_interpreter_call.interpreting")]
    ResponseCodeInterpreterCallInterpreting {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.completed")]
    ResponseCompleted {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded {
        item_id: String,
        output_index: i64,
        content_index: i64,
        part: OutputContent,
        sequence_number: i64,
    },
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone {
        item_id: String,
        output_index: i64,
        content_index: i64,
        part: OutputContent,
        sequence_number: i64,
    },
    #[serde(rename = "response.created")]
    ResponseCreated {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "error")]
    ResponseError {
        code: Option<String>,
        message: String,
        param: Option<String>,
        sequence_number: i64,
    },
    #[serde(rename = "response.file_search_call.completed")]
    ResponseFileSearchCallCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.file_search_call.in_progress")]
    ResponseFileSearchCallInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.file_search_call.searching")]
    ResponseFileSearchCallSearching {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta {
        item_id: String,
        output_index: i64,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone {
        item_id: String,
        name: String,
        output_index: i64,
        arguments: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "response.failed")]
    ResponseFailed {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "response.incomplete")]
    ResponseIncomplete {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded {
        output_index: i64,
        item: ResponseOutputItem,
        sequence_number: i64,
    },
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone {
        output_index: i64,
        item: ResponseOutputItem,
        sequence_number: i64,
    },
    #[serde(rename = "response.reasoning_summary_part.added")]
    ResponseReasoningSummaryPartAdded {
        item_id: String,
        output_index: i64,
        summary_index: i64,
        part: SummaryTextContent,
        sequence_number: i64,
    },
    #[serde(rename = "response.reasoning_summary_part.done")]
    ResponseReasoningSummaryPartDone {
        item_id: String,
        output_index: i64,
        summary_index: i64,
        part: SummaryTextContent,
        sequence_number: i64,
    },
    #[serde(rename = "response.reasoning_summary_text.delta")]
    ResponseReasoningSummaryTextDelta {
        item_id: String,
        output_index: i64,
        summary_index: i64,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.reasoning_summary_text.done")]
    ResponseReasoningSummaryTextDone {
        item_id: String,
        output_index: i64,
        summary_index: i64,
        text: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.reasoning_text.delta")]
    ResponseReasoningTextDelta {
        item_id: String,
        output_index: i64,
        content_index: i64,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.reasoning_text.done")]
    ResponseReasoningTextDone {
        item_id: String,
        output_index: i64,
        content_index: i64,
        text: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.refusal.delta")]
    ResponseRefusalDelta {
        item_id: String,
        output_index: i64,
        content_index: i64,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.refusal.done")]
    ResponseRefusalDone {
        item_id: String,
        output_index: i64,
        content_index: i64,
        refusal: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta {
        item_id: String,
        output_index: i64,
        content_index: i64,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<ResponseLogProb>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone {
        item_id: String,
        output_index: i64,
        content_index: i64,
        text: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        logprobs: Option<Vec<ResponseLogProb>>,
    },
    #[serde(rename = "response.web_search_call.completed")]
    ResponseWebSearchCallCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.web_search_call.in_progress")]
    ResponseWebSearchCallInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.web_search_call.searching")]
    ResponseWebSearchCallSearching {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.image_generation_call.completed")]
    ResponseImageGenCallCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.image_generation_call.generating")]
    ResponseImageGenCallGenerating {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.image_generation_call.in_progress")]
    ResponseImageGenCallInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.image_generation_call.partial_image")]
    ResponseImageGenCallPartialImage {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
        partial_image_index: i64,
        partial_image_b64: String,
    },
    #[serde(rename = "response.mcp_call_arguments.delta")]
    ResponseMcpCallArgumentsDelta {
        output_index: i64,
        item_id: String,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.mcp_call_arguments.done")]
    ResponseMcpCallArgumentsDone {
        output_index: i64,
        item_id: String,
        arguments: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_call.completed")]
    ResponseMcpCallCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_call.failed")]
    ResponseMcpCallFailed {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_call.in_progress")]
    ResponseMcpCallInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_list_tools.completed")]
    ResponseMcpListToolsCompleted {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_list_tools.failed")]
    ResponseMcpListToolsFailed {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.mcp_list_tools.in_progress")]
    ResponseMcpListToolsInProgress {
        output_index: i64,
        item_id: String,
        sequence_number: i64,
    },
    #[serde(rename = "response.output_text.annotation.added")]
    ResponseOutputTextAnnotationAdded {
        item_id: String,
        output_index: i64,
        content_index: i64,
        annotation_index: i64,
        annotation: Annotation,
        sequence_number: i64,
    },
    #[serde(rename = "response.queued")]
    ResponseQueued {
        response: ResponseObject,
        sequence_number: i64,
    },
    #[serde(rename = "response.custom_tool_call_input.delta")]
    ResponseCustomToolCallInputDelta {
        output_index: i64,
        item_id: String,
        delta: String,
        sequence_number: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        obfuscation: Option<String>,
    },
    #[serde(rename = "response.custom_tool_call_input.done")]
    ResponseCustomToolCallInputDone {
        output_index: i64,
        item_id: String,
        input: String,
        sequence_number: i64,
    },
}
