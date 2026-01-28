use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub use crate::openai::create_chat_completions::types::{
    Metadata, PromptCacheRetention, ReasoningEffort, ServiceTier, Verbosity,
};

pub type JsonSchema = Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Truncation {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResponseInclude {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseStreamOptions {
    /// Only valid when `stream` is true (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseUsage {
    pub input_tokens: i64,
    pub input_tokens_details: ResponseUsageInputTokensDetails,
    pub output_tokens: i64,
    pub output_tokens_details: ResponseUsageOutputTokensDetails,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseUsageInputTokensDetails {
    pub cached_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseUsageOutputTokensDetails {
    pub reasoning_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: ResponseErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseErrorCode {
    ServerError,
    RateLimitExceeded,
    InvalidPrompt,
    VectorStoreTimeout,
    InvalidImage,
    InvalidImageFormat,
    InvalidBase64Image,
    InvalidImageUrl,
    ImageTooLarge,
    ImageTooSmall,
    ImageParseError,
    ImageContentPolicyViolation,
    InvalidImageMode,
    ImageFileTooLarge,
    UnsupportedImageMediaType,
    EmptyImageFile,
    FailedToDownloadImage,
    ImageFileNotFound,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseIncompleteDetails {
    pub reason: ResponseIncompleteReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseIncompleteReason {
    MaxOutputTokens,
    ContentFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageRole {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageDetail {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputTextContent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputImageContent {
    /// Only one of `image_url` or `file_id` should be set (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    /// Defaults to `auto` when omitted (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputFileContent {
    /// Provide exactly one of `file_id`, `file_url`, or `file_data` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputTextContent {
    pub text: String,
    /// Includes annotations like file or URL citations.
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Only included when `message.output_text.logprobs` is in `include` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<LogProb>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TextContent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SummaryTextContent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningTextContent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RefusalContent {
    pub refusal: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerScreenshotContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    InputText(InputTextContent),
    InputImage(InputImageContent),
    InputFile(InputFileContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputMessageContent {
    OutputText(OutputTextContent),
    Refusal(RefusalContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputContent {
    OutputText(OutputTextContent),
    Refusal(RefusalContent),
    ReasoningText(ReasoningTextContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
    InputText(InputTextContent),
    InputImage(InputImageContent),
    InputFile(InputFileContent),
    OutputText(OutputTextContent),
    Text(TextContent),
    SummaryText(SummaryTextContent),
    ReasoningText(ReasoningTextContent),
    Refusal(RefusalContent),
    ComputerScreenshot(ComputerScreenshotContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionAndCustomToolCallOutput {
    InputText(InputTextContent),
    InputImage(InputImageContent),
    InputFile(InputFileContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Annotation {
    FileCitation {
        file_id: String,
        index: i64,
        filename: String,
    },
    UrlCitation {
        url: String,
        start_index: i64,
        end_index: i64,
        title: String,
    },
    ContainerFileCitation {
        container_id: String,
        file_id: String,
        start_index: i64,
        end_index: i64,
        filename: String,
    },
    FilePath {
        file_id: String,
        index: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LogProb {
    pub token: String,
    pub logprob: f64,
    #[serde(default)]
    pub bytes: Vec<i64>,
    #[serde(default)]
    pub top_logprobs: Vec<TopLogProb>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TopLogProb {
    pub token: String,
    pub logprob: f64,
    #[serde(default)]
    pub bytes: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseLogProb {
    pub token: String,
    pub logprob: f64,
    #[serde(default)]
    pub top_logprobs: Vec<ResponseTopLogProb>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseTopLogProb {
    pub token: String,
    pub logprob: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InputMessage {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<InputMessageType>,
    pub role: InputMessageRole,
    /// Populated when items are returned via API (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MessageStatus>,
    pub content: Vec<InputContent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OutputMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: OutputMessageType,
    /// Always `assistant` (not enforced here).
    pub role: OutputMessageRole,
    pub content: Vec<OutputMessageContent>,
    pub status: MessageStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EasyInputMessage {
    #[serde(rename = "type")]
    pub r#type: EasyInputMessageType,
    pub role: EasyInputMessageRole,
    /// For input messages, only `input_*` content parts are valid (not enforced here).
    pub content: EasyInputMessageContent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EasyInputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EasyInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EasyInputMessageContent {
    Text(String),
    Parts(Vec<InputContent>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputParam {
    Text(String),
    Items(Vec<InputItem>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum InputItem {
    EasyMessage(EasyInputMessage),
    Reference(ItemReferenceParam),
    Item(Item),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ItemReferenceParam {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<ItemReferenceType>,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemReferenceType {
    #[serde(rename = "item_reference")]
    ItemReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Instructions {
    Text(String),
    Items(Vec<InputItem>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Prompt {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<PromptVariables>,
}

/// Only string or input content values are valid (not enforced here).
pub type PromptVariables = BTreeMap<String, PromptVariable>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptVariable {
    Text(String),
    Content(InputContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    /// One of `auto`, `concise`, or `detailed` (model support varies; not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,
    /// Deprecated: use `summary` instead (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<ReasoningSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningSummary {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "concise")]
    Concise,
    #[serde(rename = "detailed")]
    Detailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ReasoningItem {
    #[serde(rename = "type")]
    pub r#type: ReasoningItemType,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default)]
    pub summary: Vec<SummaryPart>,
    #[serde(default)]
    pub content: Vec<ReasoningContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ReasoningItemStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningItemType {
    #[serde(rename = "reasoning")]
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasoningItemStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SummaryPart {
    SummaryText(SummaryTextContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReasoningContent {
    ReasoningText(ReasoningTextContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompactionSummaryItemParam {
    #[serde(rename = "type")]
    pub r#type: CompactionItemType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub encrypted_content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompactionBody {
    #[serde(rename = "type")]
    pub r#type: CompactionItemType,
    pub id: String,
    pub encrypted_content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompactionItemType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseTextParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextResponseFormatConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<Verbosity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextResponseFormatConfiguration {
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Full JSON Schema object; represented as raw JSON (not validated here).
        schema: JsonSchema,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConversationRef {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConversationParam {
    Id(String),
    Ref(ConversationRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceOptions {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoiceParam {
    Mode(ToolChoiceOptions),
    Allowed(ToolChoiceAllowed),
    BuiltIn(ToolChoiceTypes),
    Function(ToolChoiceFunction),
    MCP(ToolChoiceMCP),
    Custom(ToolChoiceCustom),
    ApplyPatch(SpecificApplyPatchParam),
    Shell(SpecificFunctionShellParam),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceAllowed {
    #[serde(rename = "type")]
    pub r#type: ToolChoiceAllowedType,
    pub mode: ToolChoiceAllowedMode,
    /// A list of tool definitions the model is allowed to call (not exhaustively validated here).
    pub tools: Vec<AllowedTool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AllowedTool {
    Function {
        name: String,
    },
    MCP {
        server_label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Custom {
        name: String,
    },
    FileSearch,
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch20250826,
    WebSearchPreview,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    ComputerUsePreview,
    CodeInterpreter,
    ImageGeneration,
    LocalShell,
    Shell,
    ApplyPatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceAllowedType {
    #[serde(rename = "allowed_tools")]
    AllowedTools,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceAllowedMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceTypes {
    #[serde(rename = "type")]
    pub r#type: ToolChoiceBuiltInType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceBuiltInType {
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    #[serde(rename = "image_generation")]
    ImageGeneration,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceFunction {
    #[serde(rename = "type")]
    pub r#type: ToolChoiceFunctionType,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceFunctionType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceCustom {
    #[serde(rename = "type")]
    pub r#type: ToolChoiceCustomType,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceCustomType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ToolChoiceMCP {
    #[serde(rename = "type")]
    pub r#type: ToolChoiceMCPType,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolChoiceMCPType {
    #[serde(rename = "mcp")]
    MCP,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SpecificApplyPatchParam {
    #[serde(rename = "type")]
    pub r#type: SpecificApplyPatchType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecificApplyPatchType {
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SpecificFunctionShellParam {
    #[serde(rename = "type")]
    pub r#type: SpecificFunctionShellType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SpecificFunctionShellType {
    #[serde(rename = "shell")]
    Shell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool {
    Function(FunctionTool),
    FileSearch(FileSearchTool),
    #[serde(rename = "web_search")]
    WebSearch(WebSearchTool),
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch20250826(WebSearchTool),
    #[serde(rename = "web_search_preview")]
    WebSearchPreview(WebSearchPreviewTool),
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311(WebSearchPreviewTool),
    ComputerUsePreview(ComputerUsePreviewTool),
    CodeInterpreter(CodeInterpreterTool),
    ImageGeneration(ImageGenTool),
    LocalShell(LocalShellTool),
    Shell(FunctionShellTool),
    Custom(CustomTool),
    MCP(MCPTool),
    ApplyPatch(ApplyPatchTool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A JSON Schema object describing parameters (not validated here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonSchema>,
    /// Only a subset of JSON Schema is supported when `strict` is true (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchTool {
    pub vector_store_ids: Vec<String>,
    /// Range is 1..=50 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<RankingOptions>,
    /// Filter object (not fully validated here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RankingOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ranker: Option<RankerVersionType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<HybridSearchOptions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RankerVersionType {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default-2024-11-15")]
    Default20241115,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct HybridSearchOptions {
    pub embedding_weight: f64,
    pub text_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Filters {
    Comparison(ComparisonFilter),
    Compound(CompoundFilter),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComparisonFilter {
    #[serde(rename = "type")]
    pub r#type: ComparisonFilterType,
    pub key: String,
    pub value: ComparisonFilterValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComparisonFilterType {
    #[serde(rename = "eq")]
    Eq,
    #[serde(rename = "ne")]
    Ne,
    #[serde(rename = "gt")]
    Gt,
    #[serde(rename = "gte")]
    Gte,
    #[serde(rename = "lt")]
    Lt,
    #[serde(rename = "lte")]
    Lte,
    #[serde(rename = "in")]
    In,
    #[serde(rename = "nin")]
    Nin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComparisonFilterValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Array(Vec<ComparisonFilterValueItem>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComparisonFilterValueItem {
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompoundFilter {
    #[serde(rename = "type")]
    pub r#type: CompoundFilterType,
    pub filters: Vec<Filters>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompoundFilterType {
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<WebSearchFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<WebSearchApproximateLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<WebSearchContextSize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchApproximateLocation {
    #[serde(rename = "type")]
    pub r#type: WebSearchLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebSearchLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebSearchContextSize {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchPreviewTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ApproximateLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApproximateLocation {
    #[serde(rename = "type")]
    pub r#type: WebSearchLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SearchContextSize {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerUsePreviewTool {
    pub environment: ComputerEnvironment,
    pub display_width: i64,
    pub display_height: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputerEnvironment {
    #[serde(rename = "windows")]
    Windows,
    #[serde(rename = "mac")]
    Mac,
    #[serde(rename = "linux")]
    Linux,
    #[serde(rename = "ubuntu")]
    Ubuntu,
    #[serde(rename = "browser")]
    Browser,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterTool {
    /// Either a container ID or a container definition (not fully validated here).
    pub container: CodeInterpreterContainer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeInterpreterContainer {
    Id(String),
    Params(CodeInterpreterContainerParams),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterContainerParams {
    #[serde(default)]
    pub file_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<ContainerMemoryLimit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContainerMemoryLimit {
    #[serde(rename = "1g")]
    G1,
    #[serde(rename = "4g")]
    G4,
    #[serde(rename = "16g")]
    G16,
    #[serde(rename = "64g")]
    G64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageGenQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageGenSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageGenOutputFormat>,
    /// Range is 0..=100 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageGenModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageGenBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<InputFidelity>,
    /// Mask object (image_url or file_id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ImageGenInputImageMask>,
    /// Range is 0..=3 (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenInputImageMask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenQuality {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenSize {
    #[serde(rename = "1024x1024")]
    S1024x1024,
    #[serde(rename = "1024x1536")]
    S1024x1536,
    #[serde(rename = "1536x1024")]
    S1536x1024,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenOutputFormat {
    #[serde(rename = "png")]
    Png,
    #[serde(rename = "webp")]
    Webp,
    #[serde(rename = "jpeg")]
    Jpeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenModeration {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "low")]
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenBackground {
    #[serde(rename = "transparent")]
    Transparent,
    #[serde(rename = "opaque")]
    Opaque,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputFidelity {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "low")]
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalShellTool {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionShellTool {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyPatchTool {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<CustomToolFormat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CustomToolFormat {
    Text,
    Grammar {
        syntax: GrammarSyntax,
        definition: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GrammarSyntax {
    #[serde(rename = "lark")]
    Lark,
    #[serde(rename = "regex")]
    Regex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPTool {
    pub server_label: String,
    /// One of `server_url` or `connector_id` must be set (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<MCPConnectorId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<MCPAllowedTools>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<MCPApprovalRequirement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPConnectorId {
    #[serde(rename = "connector_dropbox")]
    Dropbox,
    #[serde(rename = "connector_gmail")]
    Gmail,
    #[serde(rename = "connector_googlecalendar")]
    GoogleCalendar,
    #[serde(rename = "connector_googledrive")]
    GoogleDrive,
    #[serde(rename = "connector_microsoftteams")]
    MicrosoftTeams,
    #[serde(rename = "connector_outlookcalendar")]
    OutlookCalendar,
    #[serde(rename = "connector_outlookemail")]
    OutlookEmail,
    #[serde(rename = "connector_sharepoint")]
    SharePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPAllowedTools {
    Names(Vec<String>),
    Filter(MCPToolFilter),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPToolFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MCPApprovalRequirement {
    /// A single approval policy for all tools.
    Mode(MCPApprovalMode),
    /// Separate filters for always/never requiring approval.
    Rules(MCPApprovalRules),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPApprovalMode {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "never")]
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPApprovalRules {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always: Option<MCPToolFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub never: Option<MCPToolFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OutputItem {
    Message(OutputMessage),
    FileSearch(FileSearchToolCall),
    Function(FunctionToolCall),
    WebSearch(WebSearchToolCall),
    Computer(ComputerToolCall),
    Reasoning(ReasoningItem),
    Compaction(CompactionBody),
    ImageGen(ImageGenToolCall),
    CodeInterpreter(CodeInterpreterToolCall),
    LocalShell(LocalShellToolCall),
    FunctionShell(FunctionShellCall),
    FunctionShellOutput(FunctionShellCallOutput),
    ApplyPatch(ApplyPatchToolCall),
    ApplyPatchOutput(ApplyPatchToolCallOutput),
    MCPCall(MCPToolCall),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    CustomToolCall(CustomToolCall),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Item {
    InputMessage(InputMessage),
    OutputMessage(OutputMessage),
    FileSearch(FileSearchToolCall),
    Computer(ComputerToolCall),
    ComputerOutput(ComputerCallOutputItemParam),
    WebSearch(WebSearchToolCall),
    Function(FunctionToolCall),
    FunctionOutput(FunctionCallOutputItemParam),
    Reasoning(ReasoningItem),
    Compaction(CompactionSummaryItemParam),
    ImageGen(ImageGenToolCall),
    CodeInterpreter(CodeInterpreterToolCall),
    LocalShell(LocalShellToolCall),
    LocalShellOutput(LocalShellToolCallOutput),
    FunctionShell(FunctionShellCallItemParam),
    FunctionShellOutput(FunctionShellCallOutputItemParam),
    ApplyPatch(ApplyPatchToolCallItemParam),
    ApplyPatchOutput(ApplyPatchToolCallOutputItemParam),
    MCPListTools(MCPListTools),
    MCPApprovalRequest(MCPApprovalRequest),
    MCPApprovalResponse(MCPApprovalResponse),
    MCPCall(MCPToolCall),
    CustomToolCallOutput(CustomToolCallOutput),
    CustomToolCall(CustomToolCall),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: FileSearchToolCallType,
    pub status: FileSearchToolCallStatus,
    pub queries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<FileSearchResult>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileSearchToolCallType {
    #[serde(rename = "file_search_call")]
    FileSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileSearchToolCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "searching")]
    Searching,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FileSearchResult {
    pub file_id: String,
    pub text: String,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributes: Option<VectorStoreFileAttributes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
}

pub type VectorStoreFileAttributes = BTreeMap<String, VectorStoreFileAttributeValue>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VectorStoreFileAttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: WebSearchToolCallType,
    pub status: WebSearchToolCallStatus,
    pub action: WebSearchAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebSearchToolCallType {
    #[serde(rename = "web_search_call")]
    WebSearchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebSearchToolCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "searching")]
    Searching,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebSearchAction {
    Search(WebSearchActionSearch),
    OpenPage(WebSearchActionOpenPage),
    Find(WebSearchActionFind),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchActionSearch {
    /// Deprecated in favor of `queries` (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queries: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<WebSearchSource>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchSource {
    #[serde(rename = "type")]
    pub r#type: WebSearchSourceType,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchActionOpenPage {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct WebSearchActionFind {
    pub url: String,
    pub pattern: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerToolCall {
    #[serde(rename = "type")]
    pub r#type: ComputerToolCallType,
    pub id: String,
    pub call_id: String,
    pub action: ComputerAction,
    pub pending_safety_checks: Vec<ComputerCallSafetyCheckParam>,
    pub status: ComputerCallStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputerToolCallType {
    #[serde(rename = "computer_call")]
    ComputerCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputerCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerCallSafetyCheckParam {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComputerAction {
    Click(ClickAction),
    DoubleClick(DoubleClickAction),
    Drag(DragAction),
    Keypress(KeyPressAction),
    Move(MoveAction),
    Screenshot(ScreenshotAction),
    Scroll(ScrollAction),
    Type(TypeAction),
    Wait(WaitAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClickButtonType {
    #[serde(rename = "left")]
    Left,
    #[serde(rename = "right")]
    Right,
    #[serde(rename = "wheel")]
    Wheel,
    #[serde(rename = "back")]
    Back,
    #[serde(rename = "forward")]
    Forward,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ClickAction {
    pub button: ClickButtonType,
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DoubleClickAction {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DragAction {
    pub path: Vec<DragPoint>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DragPoint {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct KeyPressAction {
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MoveAction {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotAction {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScrollAction {
    pub x: i64,
    pub y: i64,
    pub scroll_x: i64,
    pub scroll_y: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TypeAction {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitAction {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerCallOutputItemParam {
    #[serde(rename = "type")]
    pub r#type: ComputerCallOutputItemType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: ComputerScreenshotImage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<ComputerCallSafetyCheckParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FunctionCallItemStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputerCallOutputItemType {
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ComputerScreenshotImage {
    #[serde(rename = "type")]
    pub r#type: ComputerScreenshotImageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComputerScreenshotImageType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionCallItemStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionToolCall {
    #[serde(rename = "type")]
    pub r#type: FunctionToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FunctionCallItemStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionToolCallType {
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolCallOutput {
    Text(String),
    Content(Vec<FunctionAndCustomToolCallOutput>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionCallOutputItemParam {
    #[serde(rename = "type")]
    pub r#type: FunctionCallOutputItemType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: ToolCallOutput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FunctionCallItemStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionCallOutputItemType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCall {
    #[serde(rename = "type")]
    pub r#type: CustomToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub name: String,
    pub input: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CustomToolCallType {
    #[serde(rename = "custom_tool_call")]
    CustomToolCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CustomToolCallOutput {
    #[serde(rename = "type")]
    pub r#type: CustomToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: ToolCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CustomToolCallOutputType {
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterToolCall {
    #[serde(rename = "type")]
    pub r#type: CodeInterpreterToolCallType,
    pub id: String,
    pub status: CodeInterpreterToolCallStatus,
    pub container_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<CodeInterpreterOutput>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeInterpreterToolCallType {
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeInterpreterToolCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
    #[serde(rename = "interpreting")]
    Interpreting,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeInterpreterOutput {
    Logs(CodeInterpreterOutputLogs),
    Image(CodeInterpreterOutputImage),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterOutputLogs {
    pub logs: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CodeInterpreterOutputImage {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellToolCall {
    #[serde(rename = "type")]
    pub r#type: LocalShellToolCallType,
    pub id: String,
    pub call_id: String,
    pub action: LocalShellExecAction,
    pub status: LocalShellCallStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalShellToolCallType {
    #[serde(rename = "local_shell_call")]
    LocalShellCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellToolCallOutput {
    #[serde(rename = "type")]
    pub r#type: LocalShellToolCallOutputType,
    /// The ID is populated when returned by the API (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<LocalShellCallStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalShellToolCallOutputType {
    #[serde(rename = "local_shell_call_output")]
    LocalShellCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalShellCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalShellExecAction {
    #[serde(rename = "type")]
    pub r#type: LocalShellExecActionType,
    pub command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    pub env: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalShellExecActionType {
    #[serde(rename = "exec")]
    Exec,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCall {
    #[serde(rename = "type")]
    pub r#type: FunctionShellCallType,
    pub id: String,
    pub call_id: String,
    pub action: FunctionShellAction,
    pub status: FunctionShellCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionShellCallType {
    #[serde(rename = "shell_call")]
    ShellCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionShellCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCallItemParam {
    #[serde(rename = "type")]
    pub r#type: FunctionShellCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub action: FunctionShellAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<FunctionShellCallStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellAction {
    pub commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCallOutput {
    #[serde(rename = "type")]
    pub r#type: FunctionShellCallOutputType,
    pub id: String,
    pub call_id: String,
    pub output: Vec<FunctionShellCallOutputContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCallOutputItemParam {
    #[serde(rename = "type")]
    pub r#type: FunctionShellCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub output: Vec<FunctionShellCallOutputContentParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FunctionShellCallOutputType {
    #[serde(rename = "shell_call_output")]
    ShellCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCallOutputContent {
    pub stdout: String,
    pub stderr: String,
    pub outcome: FunctionShellCallOutputOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FunctionShellCallOutputContentParam {
    pub stdout: String,
    pub stderr: String,
    pub outcome: FunctionShellCallOutputOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FunctionShellCallOutputOutcome {
    Timeout,
    Exit { exit_code: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchToolCall {
    #[serde(rename = "type")]
    pub r#type: ApplyPatchToolCallType,
    pub id: String,
    pub call_id: String,
    pub status: ApplyPatchCallStatus,
    pub operation: ApplyPatchOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchToolCallItemParam {
    #[serde(rename = "type")]
    pub r#type: ApplyPatchToolCallType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub status: ApplyPatchCallStatus,
    pub operation: ApplyPatchOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplyPatchToolCallType {
    #[serde(rename = "apply_patch_call")]
    ApplyPatchCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplyPatchCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApplyPatchOperation {
    CreateFile { path: String, diff: String },
    DeleteFile { path: String },
    UpdateFile { path: String, diff: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchToolCallOutput {
    #[serde(rename = "type")]
    pub r#type: ApplyPatchToolCallOutputType,
    pub id: String,
    pub call_id: String,
    pub status: ApplyPatchCallOutputStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ApplyPatchToolCallOutputItemParam {
    #[serde(rename = "type")]
    pub r#type: ApplyPatchToolCallOutputType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    pub status: ApplyPatchCallOutputStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplyPatchToolCallOutputType {
    #[serde(rename = "apply_patch_call_output")]
    ApplyPatchCallOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApplyPatchCallOutputStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageGenToolCall {
    #[serde(rename = "type")]
    pub r#type: ImageGenToolCallType,
    pub id: String,
    pub status: ImageGenToolCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenToolCallType {
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageGenToolCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "generating")]
    Generating,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPToolCall {
    #[serde(rename = "type")]
    pub r#type: MCPToolCallType,
    pub id: String,
    pub server_label: String,
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub status: MCPToolCallStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPToolCallType {
    #[serde(rename = "mcp_call")]
    MCPCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPToolCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
    #[serde(rename = "calling")]
    Calling,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPListTools {
    #[serde(rename = "type")]
    pub r#type: MCPListToolsType,
    pub id: String,
    pub server_label: String,
    pub tools: Vec<MCPListToolsTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPListToolsType {
    #[serde(rename = "mcp_list_tools")]
    MCPListTools,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPListToolsTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the tool input.
    pub input_schema: JsonSchema,
    /// Arbitrary annotations (not validated here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<JsonSchema>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPApprovalRequest {
    #[serde(rename = "type")]
    pub r#type: MCPApprovalRequestType,
    pub id: String,
    pub server_label: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPApprovalRequestType {
    #[serde(rename = "mcp_approval_request")]
    MCPApprovalRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MCPApprovalResponse {
    #[serde(rename = "type")]
    pub r#type: MCPApprovalResponseType,
    /// Populated when returned by the API (not enforced here).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MCPApprovalResponseType {
    #[serde(rename = "mcp_approval_response")]
    MCPApprovalResponse,
}
