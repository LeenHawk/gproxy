use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use crate::openai::types::{
    HttpMethod, OpenAiApiError, OpenAiApiErrorResponse, OpenAiResponseHeaders,
};

/// JSON object type used for fields documented as `map[unknown]`.
pub type JsonObject = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseConversation {
    Id(String),
    Param(ResponseConversationParam),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseConversationParam {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInput {
    Text(String),
    Items(Vec<ResponseInputItem>),
}

/// Input item union accepted by count-tokens endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInputItem {
    Message(ResponseInputMessage),
    OutputMessage(ResponseOutputMessage),
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
    ImageGenerationCall(ResponseImageGenerationCall),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputMessage {
    pub content: ResponseInputMessageContent,
    pub role: ResponseInputMessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponseMessagePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseInputMessageType>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInputMessageContent {
    Text(String),
    List(Vec<ResponseInputContent>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInputContent {
    Text(ResponseInputText),
    Image(ResponseInputImage),
    File(ResponseInputFile),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseInputText {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseInputTextType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputTextType {
    #[serde(rename = "input_text")]
    InputText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputImage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ResponseInputImageDetail>,
    #[serde(rename = "type")]
    pub type_: ResponseInputImageType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputImageDetail {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "original")]
    Original,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputImageType {
    #[serde(rename = "input_image")]
    InputImage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInputFile {
    #[serde(rename = "type")]
    pub type_: ResponseInputFileType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ResponseInputFileDetail>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputFileType {
    #[serde(rename = "input_file")]
    InputFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputFileDetail {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputMessageRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "developer")]
    Developer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMessagePhase {
    #[serde(rename = "commentary")]
    Commentary,
    #[serde(rename = "final_answer")]
    FinalAnswer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseItemStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "incomplete")]
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputMessage {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub content: Vec<ResponseOutputContent>,
    pub role: ResponseOutputMessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<ResponseMessagePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseOutputMessageType>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutputContent {
    Text(ResponseOutputText),
    Refusal(ResponseOutputRefusal),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputText {
    #[serde(default)]
    pub annotations: Vec<ResponseOutputTextAnnotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<ResponseOutputTokenLogprob>>,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseOutputTextType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseOutputTextAnnotation {
    FileCitation(ResponseFileCitation),
    UrlCitation(ResponseUrlCitation),
    ContainerFileCitation(ResponseContainerFileCitation),
    FilePath(ResponseFilePath),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseFileCitation {
    pub file_id: String,
    pub filename: String,
    pub index: u64,
    #[serde(rename = "type")]
    pub type_: ResponseFileCitationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFileCitationType {
    #[serde(rename = "file_citation")]
    FileCitation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseUrlCitation {
    pub end_index: u64,
    pub start_index: u64,
    pub title: String,
    #[serde(rename = "type")]
    pub type_: ResponseUrlCitationType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseUrlCitationType {
    #[serde(rename = "url_citation")]
    UrlCitation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseContainerFileCitation {
    pub container_id: String,
    pub end_index: u64,
    pub file_id: String,
    pub filename: String,
    pub start_index: u64,
    #[serde(rename = "type")]
    pub type_: ResponseContainerFileCitationType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContainerFileCitationType {
    #[serde(rename = "container_file_citation")]
    ContainerFileCitation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseFilePath {
    pub file_id: String,
    pub index: u64,
    #[serde(rename = "type")]
    pub type_: ResponseFilePathType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFilePathType {
    #[serde(rename = "file_path")]
    FilePath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputTokenLogprob {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    pub logprob: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_logprobs: Vec<ResponseOutputTopLogprob>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseOutputTopLogprob {
    pub token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    pub logprob: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputTextType {
    #[serde(rename = "output_text")]
    OutputText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseOutputRefusal {
    pub refusal: String,
    #[serde(rename = "type")]
    pub type_: ResponseOutputRefusalType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputRefusalType {
    #[serde(rename = "refusal")]
    Refusal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputMessageRole {
    #[serde(rename = "assistant")]
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseOutputMessageType {
    #[serde(rename = "message")]
    Message,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_output_message_type_and_status_are_optional() {
        let message: ResponseOutputMessage = serde_json::from_value(serde_json::json!({
            "role": "assistant",
            "content": [{ "type": "output_text", "text": "hello" }]
        }))
        .expect("assistant output message without optional fields should deserialize");

        let value = serde_json::to_value(message).expect("message serializes");
        assert!(value.get("status").is_none(), "status should stay absent");
        assert!(value.get("type").is_none(), "type should stay absent");
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFileSearchToolCall {
    pub id: String,
    pub queries: Vec<String>,
    pub status: ResponseFileSearchToolCallStatus,
    #[serde(rename = "type")]
    pub type_: ResponseFileSearchToolCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Vec<ResponseFileSearchResult>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFileSearchToolCallStatus {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFileSearchToolCallType {
    #[serde(rename = "file_search_call")]
    FileSearchCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseFileSearchResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, ResponseAttributeValue>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseAttributeValue {
    String(String),
    Number(f64),
    Boolean(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseComputerToolCall {
    pub id: String,
    pub action: ResponseComputerAction,
    pub call_id: String,
    pub pending_safety_checks: Vec<ResponseSafetyCheck>,
    pub status: ResponseItemStatus,
    #[serde(rename = "type")]
    pub type_: ResponseComputerToolCallType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseComputerAction {
    #[serde(rename = "click")]
    Click {
        button: ResponseComputerMouseButton,
        x: f64,
        y: f64,
    },
    #[serde(rename = "double_click")]
    DoubleClick { x: f64, y: f64 },
    #[serde(rename = "drag")]
    Drag { path: Vec<ResponseComputerPoint> },
    #[serde(rename = "keypress")]
    Keypress { keys: Vec<String> },
    #[serde(rename = "move")]
    Move { x: f64, y: f64 },
    #[serde(rename = "screenshot")]
    Screenshot,
    #[serde(rename = "scroll")]
    Scroll {
        scroll_x: f64,
        scroll_y: f64,
        x: f64,
        y: f64,
    },
    #[serde(rename = "type")]
    Type { text: String },
    #[serde(rename = "wait")]
    Wait,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerMouseButton {
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
pub struct ResponseComputerPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseSafetyCheck {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerToolCallType {
    #[serde(rename = "computer_call")]
    ComputerCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseComputerCallOutput {
    pub call_id: String,
    pub output: ResponseComputerToolCallOutputScreenshot,
    #[serde(rename = "type")]
    pub type_: ResponseComputerCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acknowledged_safety_checks: Option<Vec<ResponseSafetyCheck>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseComputerToolCallOutputScreenshot {
    #[serde(rename = "type")]
    pub type_: ResponseComputerToolCallOutputScreenshotType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerToolCallOutputScreenshotType {
    #[serde(rename = "computer_screenshot")]
    ComputerScreenshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerCallOutputType {
    #[serde(rename = "computer_call_output")]
    ComputerCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolSearchCall {
    pub id: String,
    pub arguments: Value,
    pub call_id: String,
    pub execution: ResponseToolSearchExecution,
    pub status: ResponseItemStatus,
    #[serde(rename = "type")]
    pub type_: ResponseToolSearchCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolSearchOutput {
    pub id: String,
    pub call_id: String,
    pub execution: ResponseToolSearchExecution,
    pub status: ResponseItemStatus,
    pub tools: Vec<ResponseTool>,
    #[serde(rename = "type")]
    pub type_: ResponseToolSearchOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolSearchExecution {
    #[serde(rename = "server")]
    Server,
    #[serde(rename = "client")]
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolSearchCallType {
    #[serde(rename = "tool_search_call")]
    ToolSearchCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolSearchOutputType {
    #[serde(rename = "tool_search_output")]
    ToolSearchOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionWebSearch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub action: ResponseFunctionWebSearchAction,
    pub status: ResponseFunctionWebSearchStatus,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionWebSearchType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseFunctionWebSearchAction {
    #[serde(rename = "search")]
    Search {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        queries: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sources: Option<Vec<ResponseFunctionWebSearchSource>>,
    },
    #[serde(rename = "open_page")]
    OpenPage {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    #[serde(rename = "find_in_page")]
    FindInPage { pattern: String, url: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionWebSearchSource {
    #[serde(rename = "type")]
    pub type_: ResponseFunctionWebSearchSourceType,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionWebSearchSourceType {
    #[serde(rename = "url")]
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionWebSearchStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "searching")]
    Searching,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionWebSearchType {
    #[serde(rename = "web_search_call")]
    WebSearchCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionToolCall {
    pub arguments: String,
    pub call_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionToolCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionToolCallType {
    #[serde(rename = "function_call")]
    FunctionCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionCallOutput {
    pub call_id: String,
    pub output: ResponseFunctionCallOutputContent,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFunctionCallOutputContent {
    Text(String),
    Content(Vec<ResponseInputContent>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionCallOutputType {
    #[serde(rename = "function_call_output")]
    FunctionCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Vec<ResponseSummaryTextContent>,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningItemType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<ResponseReasoningTextContent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseSummaryTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseSummaryTextContentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseSummaryTextContentType {
    #[serde(rename = "summary_text")]
    SummaryText,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseReasoningTextContent {
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseReasoningTextContentType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningTextContentType {
    #[serde(rename = "reasoning_text")]
    ReasoningText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningItemType {
    #[serde(rename = "reasoning")]
    Reasoning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCompactionItemParam {
    pub encrypted_content: String,
    #[serde(rename = "type")]
    pub type_: ResponseCompactionItemType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCompactionItemType {
    #[serde(rename = "compaction")]
    Compaction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseImageGenerationCall {
    pub id: String,
    pub result: String,
    pub status: ResponseImageGenerationCallStatus,
    #[serde(rename = "type")]
    pub type_: ResponseImageGenerationCallType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "generating")]
    Generating,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationCallType {
    #[serde(rename = "image_generation_call")]
    ImageGenerationCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCodeInterpreterToolCall {
    pub id: String,
    pub code: String,
    pub container_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs: Option<Vec<ResponseCodeInterpreterOutputItem>>,
    pub status: ResponseCodeInterpreterToolCallStatus,
    #[serde(rename = "type")]
    pub type_: ResponseCodeInterpreterToolCallType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseCodeInterpreterOutputItem {
    #[serde(rename = "logs")]
    Logs { logs: String },
    #[serde(rename = "image")]
    Image { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCodeInterpreterToolCallStatus {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCodeInterpreterToolCallType {
    #[serde(rename = "code_interpreter_call")]
    CodeInterpreterCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseLocalShellCall {
    pub id: String,
    pub action: ResponseLocalShellExecAction,
    pub call_id: String,
    pub status: ResponseItemStatus,
    #[serde(rename = "type")]
    pub type_: ResponseLocalShellCallType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseLocalShellExecAction {
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(rename = "type")]
    pub type_: ResponseLocalShellExecActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLocalShellExecActionType {
    #[serde(rename = "exec")]
    Exec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLocalShellCallType {
    #[serde(rename = "local_shell_call")]
    LocalShellCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseLocalShellCallOutput {
    pub id: String,
    pub output: String,
    #[serde(rename = "type")]
    pub type_: ResponseLocalShellCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLocalShellCallOutputType {
    #[serde(rename = "local_shell_call_output")]
    LocalShellCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellCall {
    pub action: ResponseShellCallAction,
    pub call_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseShellCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ResponseShellEnvironment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellCallAction {
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseShellCallType {
    #[serde(rename = "shell_call")]
    ShellCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseShellEnvironment {
    Local(ResponseLocalEnvironment),
    ContainerReference(ResponseContainerReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseLocalEnvironment {
    #[serde(rename = "type")]
    pub type_: ResponseLocalEnvironmentType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<ResponseLocalSkill>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLocalEnvironmentType {
    #[serde(rename = "local")]
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseLocalSkill {
    pub description: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseContainerReference {
    pub container_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseContainerReferenceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContainerReferenceType {
    #[serde(rename = "container_reference")]
    ContainerReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellCallOutput {
    pub call_id: String,
    pub output: Vec<ResponseFunctionShellCallOutputContent>,
    #[serde(rename = "type")]
    pub type_: ResponseShellCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseItemStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionShellCallOutputContent {
    pub outcome: ResponseShellCallOutcome,
    pub stderr: String,
    pub stdout: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseShellCallOutcome {
    #[serde(rename = "timeout")]
    Timeout,
    #[serde(rename = "exit")]
    Exit { exit_code: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseShellCallOutputType {
    #[serde(rename = "shell_call_output")]
    ShellCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseApplyPatchCall {
    pub call_id: String,
    pub operation: ResponseApplyPatchOperation,
    pub status: ResponseApplyPatchCallStatus,
    #[serde(rename = "type")]
    pub type_: ResponseApplyPatchCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseApplyPatchOperation {
    #[serde(rename = "create_file")]
    CreateFile { diff: String, path: String },
    #[serde(rename = "delete_file")]
    DeleteFile { path: String },
    #[serde(rename = "update_file")]
    UpdateFile { diff: String, path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApplyPatchCallStatus {
    #[serde(rename = "in_progress")]
    InProgress,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApplyPatchCallType {
    #[serde(rename = "apply_patch_call")]
    ApplyPatchCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseApplyPatchCallOutput {
    pub call_id: String,
    pub status: ResponseApplyPatchCallOutputStatus,
    #[serde(rename = "type")]
    pub type_: ResponseApplyPatchCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApplyPatchCallOutputStatus {
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApplyPatchCallOutputType {
    #[serde(rename = "apply_patch_call_output")]
    ApplyPatchCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpListTools {
    pub id: String,
    pub server_label: String,
    pub tools: Vec<ResponseMcpToolDescriptor>,
    #[serde(rename = "type")]
    pub type_: ResponseMcpListToolsType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpToolDescriptor {
    pub input_schema: Value,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpListToolsType {
    #[serde(rename = "mcp_list_tools")]
    McpListTools,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseMcpApprovalRequest {
    pub id: String,
    pub arguments: String,
    pub name: String,
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpApprovalRequestType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpApprovalRequestType {
    #[serde(rename = "mcp_approval_request")]
    McpApprovalRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpApprovalResponse {
    pub approval_request_id: String,
    pub approve: bool,
    #[serde(rename = "type")]
    pub type_: ResponseMcpApprovalResponseType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpApprovalResponseType {
    #[serde(rename = "mcp_approval_response")]
    McpApprovalResponse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpCall {
    pub id: String,
    pub arguments: String,
    pub name: String,
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ResponseToolCallStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpCallType {
    #[serde(rename = "mcp_call")]
    McpCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolCallStatus {
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
pub struct ResponseCustomToolCallOutput {
    pub call_id: String,
    pub output: ResponseCustomToolCallOutputContent,
    #[serde(rename = "type")]
    pub type_: ResponseCustomToolCallOutputType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseCustomToolCallOutputContent {
    Text(String),
    Content(Vec<ResponseInputContent>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolCallOutputType {
    #[serde(rename = "custom_tool_call_output")]
    CustomToolCallOutput,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCustomToolCall {
    pub call_id: String,
    pub input: String,
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseCustomToolCallType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolCallType {
    #[serde(rename = "custom_tool_call")]
    CustomToolCall,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseItemReference {
    pub id: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseItemReferenceType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseItemReferenceType {
    #[serde(rename = "item_reference")]
    ItemReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseReasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ResponseReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<ResponseReasoningSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ResponseReasoningSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningEffort {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "minimal")]
    Minimal,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "xhigh")]
    XHigh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseReasoningSummary {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "concise")]
    Concise,
    #[serde(rename = "detailed")]
    Detailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseTextConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ResponseTextFormatConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<ResponseTextVerbosity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseTextFormatConfig {
    Text(ResponseFormatText),
    JsonSchema(ResponseFormatTextJsonSchemaConfig),
    JsonObject(ResponseFormatJsonObject),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseFormatText {
    #[serde(rename = "type")]
    pub type_: ResponseFormatTextType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormatTextType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFormatTextJsonSchemaConfig {
    pub name: String,
    pub schema: JsonObject,
    #[serde(rename = "type")]
    pub type_: ResponseFormatTextJsonSchemaConfigType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormatTextJsonSchemaConfigType {
    #[serde(rename = "json_schema")]
    JsonSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseFormatJsonObject {
    #[serde(rename = "type")]
    pub type_: ResponseFormatJsonObjectType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFormatJsonObjectType {
    #[serde(rename = "json_object")]
    JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseTextVerbosity {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseToolChoice {
    Options(ResponseToolChoiceOptions),
    Allowed(ResponseToolChoiceAllowed),
    Types(ResponseToolChoiceTypes),
    Function(ResponseToolChoiceFunction),
    Mcp(ResponseToolChoiceMcp),
    Custom(ResponseToolChoiceCustom),
    ApplyPatch(ResponseToolChoiceApplyPatch),
    Shell(ResponseToolChoiceShell),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceOptions {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolChoiceAllowed {
    pub mode: ResponseToolChoiceAllowedMode,
    pub tools: Vec<JsonObject>,
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceAllowedType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceAllowedMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "required")]
    Required,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceAllowedType {
    #[serde(rename = "allowed_tools")]
    AllowedTools,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseToolChoiceTypes {
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceBuiltinType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceBuiltinType {
    #[serde(rename = "file_search")]
    FileSearch,
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "computer")]
    Computer,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
    #[serde(rename = "computer_use")]
    ComputerUse,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    #[serde(rename = "image_generation")]
    ImageGeneration,
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseToolChoiceFunction {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceFunctionType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceFunctionType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolChoiceMcp {
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceMcpType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceMcpType {
    #[serde(rename = "mcp")]
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseToolChoiceCustom {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceCustomType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceCustomType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseToolChoiceApplyPatch {
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceApplyPatchType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceApplyPatchType {
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseToolChoiceShell {
    #[serde(rename = "type")]
    pub type_: ResponseToolChoiceShellType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolChoiceShellType {
    #[serde(rename = "shell")]
    Shell,
}

/// Tool union accepted by the endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseTool {
    Function(ResponseFunctionTool),
    FileSearch(ResponseFileSearchTool),
    Computer(ResponseComputerTool),
    WebSearch(ResponseWebSearchTool),
    Namespace(ResponseNamespaceTool),
    ToolSearch(ResponseToolSearchTool),
    Mcp(ResponseMcpTool),
    CodeInterpreter(ResponseCodeInterpreterTool),
    ImageGeneration(ResponseImageGenerationTool),
    LocalShell(ResponseLocalShellTool),
    Shell(ResponseFunctionShellTool),
    Custom(ResponseCustomTool),
    WebSearchPreview(ResponseWebSearchPreviewTool),
    ApplyPatch(ResponseApplyPatchTool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionTool {
    pub name: String,
    pub parameters: JsonObject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionToolType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFileSearchTool {
    #[serde(rename = "type")]
    pub type_: ResponseFileSearchToolType,
    pub vector_store_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<ResponseFileSearchFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_num_results: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranking_options: Option<ResponseFileSearchRankingOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFileSearchToolType {
    #[serde(rename = "file_search")]
    FileSearch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseFileSearchFilter {
    Comparison(ResponseComparisonFilter),
    Compound(ResponseCompoundFilter),
    /// Explicitly documented by upstream as `unknown` for nested compound filters.
    Unknown(JsonObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseComparisonFilter {
    pub key: String,
    #[serde(rename = "type")]
    pub type_: ResponseComparisonOperator,
    pub value: ResponseComparisonValue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComparisonOperator {
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
pub enum ResponseComparisonValue {
    String(String),
    Number(f64),
    Boolean(bool),
    List(Vec<ResponseComparisonScalar>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseComparisonScalar {
    String(String),
    Number(f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCompoundFilter {
    pub filters: Vec<ResponseFileSearchFilter>,
    #[serde(rename = "type")]
    pub type_: ResponseCompoundFilterType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCompoundFilterType {
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseFileSearchRankingOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hybrid_search: Option<ResponseHybridSearchWeights>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ranker: Option<ResponseFileSearchRanker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseHybridSearchWeights {
    pub embedding_weight: f64,
    pub text_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFileSearchRanker {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "default-2024-11-15")]
    Default20241115,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseComputerTool {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_width: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ResponseComputerEnvironment>,
    #[serde(rename = "type")]
    pub type_: ResponseComputerToolType,
}

impl ResponseComputerTool {
    pub const DEFAULT_DISPLAY_HEIGHT: u64 = 1024;
    pub const DEFAULT_DISPLAY_WIDTH: u64 = 1024;

    pub fn display_height_or_default(&self) -> u64 {
        self.display_height.unwrap_or(Self::DEFAULT_DISPLAY_HEIGHT)
    }

    pub fn display_width_or_default(&self) -> u64 {
        self.display_width.unwrap_or(Self::DEFAULT_DISPLAY_WIDTH)
    }

    pub fn environment_or_default(&self) -> ResponseComputerEnvironment {
        self.environment
            .clone()
            .unwrap_or(ResponseComputerEnvironment::Browser)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerEnvironment {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseComputerToolType {
    #[serde(rename = "computer")]
    Computer,
    #[serde(rename = "computer_use_preview")]
    ComputerUsePreview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseNamespaceTool {
    pub description: String,
    pub name: String,
    pub tools: Vec<ResponseNamespaceToolItem>,
    #[serde(rename = "type")]
    pub type_: ResponseNamespaceToolType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseNamespaceToolType {
    #[serde(rename = "namespace")]
    Namespace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseNamespaceToolItem {
    Function(ResponseNamespaceFunctionTool),
    Custom(ResponseCustomTool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseNamespaceFunctionTool {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseNamespaceFunctionToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<JsonObject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseNamespaceFunctionToolType {
    #[serde(rename = "function")]
    Function,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseToolSearchTool {
    #[serde(rename = "type")]
    pub type_: ResponseToolSearchToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ResponseToolSearchExecution>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseToolSearchToolType {
    #[serde(rename = "tool_search")]
    ToolSearch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebSearchTool {
    #[serde(rename = "type")]
    pub type_: ResponseWebSearchToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<ResponseWebSearchFilters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<ResponseWebSearchContextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ResponseApproximateLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseWebSearchToolType {
    #[serde(rename = "web_search")]
    WebSearch,
    #[serde(rename = "web_search_2025_08_26")]
    WebSearch20250826,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseWebSearchFilters {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseWebSearchContextSize {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseApproximateLocation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<ResponseApproximateLocationType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApproximateLocationType {
    #[serde(rename = "approximate")]
    Approximate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseMcpTool {
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<ResponseMcpAllowedTools>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<ResponseMcpConnectorId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<ResponseMcpRequireApproval>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpToolType {
    #[serde(rename = "mcp")]
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseMcpAllowedTools {
    ToolNames(Vec<String>),
    Filter(ResponseMcpToolFilter),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseMcpToolFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpConnectorId {
    #[serde(rename = "connector_dropbox")]
    ConnectorDropbox,
    #[serde(rename = "connector_gmail")]
    ConnectorGmail,
    #[serde(rename = "connector_googlecalendar")]
    ConnectorGoogleCalendar,
    #[serde(rename = "connector_googledrive")]
    ConnectorGoogleDrive,
    #[serde(rename = "connector_microsoftteams")]
    ConnectorMicrosoftTeams,
    #[serde(rename = "connector_outlookcalendar")]
    ConnectorOutlookCalendar,
    #[serde(rename = "connector_outlookemail")]
    ConnectorOutlookEmail,
    #[serde(rename = "connector_sharepoint")]
    ConnectorSharePoint,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseMcpRequireApproval {
    Filter(ResponseMcpToolApprovalFilter),
    Setting(ResponseMcpToolApprovalSetting),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseMcpToolApprovalFilter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always: Option<ResponseMcpToolFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub never: Option<ResponseMcpToolFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseMcpToolApprovalSetting {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "never")]
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCodeInterpreterTool {
    pub container: ResponseCodeInterpreterContainer,
    #[serde(rename = "type")]
    pub type_: ResponseCodeInterpreterToolType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseCodeInterpreterContainer {
    Id(String),
    Auto(ResponseCodeInterpreterToolAuto),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCodeInterpreterToolAuto {
    #[serde(rename = "type")]
    pub type_: ResponseCodeInterpreterToolAutoType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<ResponseContainerMemoryLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<ResponseContainerNetworkPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCodeInterpreterToolAutoType {
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContainerMemoryLimit {
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
#[serde(untagged)]
pub enum ResponseContainerNetworkPolicy {
    Disabled(ResponseContainerNetworkPolicyDisabled),
    Allowlist(ResponseContainerNetworkPolicyAllowlist),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseContainerNetworkPolicyDisabled {
    #[serde(rename = "type")]
    pub type_: ResponseContainerNetworkPolicyDisabledType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContainerNetworkPolicyDisabledType {
    #[serde(rename = "disabled")]
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseContainerNetworkPolicyAllowlist {
    pub allowed_domains: Vec<String>,
    #[serde(rename = "type")]
    pub type_: ResponseContainerNetworkPolicyAllowlistType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain_secrets: Option<Vec<ResponseContainerNetworkPolicyDomainSecret>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseContainerNetworkPolicyAllowlistType {
    #[serde(rename = "allowlist")]
    Allowlist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseContainerNetworkPolicyDomainSecret {
    pub domain: String,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCodeInterpreterToolType {
    #[serde(rename = "code_interpreter")]
    CodeInterpreter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseImageGenerationTool {
    #[serde(rename = "type")]
    pub type_: ResponseImageGenerationToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ResponseImageGenerationAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<ResponseImageGenerationBackground>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<ResponseImageGenerationInputFidelity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_image_mask: Option<ResponseImageGenerationInputImageMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ResponseImageGenerationModel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ResponseImageGenerationModeration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ResponseImageGenerationOutputFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<ResponseImageGenerationQuality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<ResponseImageGenerationSize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationToolType {
    #[serde(rename = "image_generation")]
    ImageGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationAction {
    #[serde(rename = "generate")]
    Generate,
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationBackground {
    #[serde(rename = "transparent")]
    Transparent,
    #[serde(rename = "opaque")]
    Opaque,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationInputFidelity {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "low")]
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ResponseImageGenerationInputImageMask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseImageGenerationModel {
    Known(ResponseImageGenerationModelKnown),
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationModelKnown {
    #[serde(rename = "gpt-image-1")]
    GptImage1,
    #[serde(rename = "gpt-image-1-mini")]
    GptImage1Mini,
    #[serde(rename = "gpt-image-1.5")]
    GptImage15,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationModeration {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "low")]
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationOutputFormat {
    #[serde(rename = "png")]
    Png,
    #[serde(rename = "webp")]
    Webp,
    #[serde(rename = "jpeg")]
    Jpeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationQuality {
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "high")]
    High,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseImageGenerationSize {
    #[serde(rename = "1024x1024")]
    S1024x1024,
    #[serde(rename = "1024x1536")]
    S1024x1536,
    #[serde(rename = "1536x1024")]
    S1536x1024,
    #[serde(rename = "auto")]
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseLocalShellTool {
    #[serde(rename = "type")]
    pub type_: ResponseLocalShellToolType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseLocalShellToolType {
    #[serde(rename = "local_shell")]
    LocalShell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseFunctionShellTool {
    #[serde(rename = "type")]
    pub type_: ResponseFunctionShellToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<ResponseShellToolEnvironment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseFunctionShellToolType {
    #[serde(rename = "shell")]
    Shell,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseShellToolEnvironment {
    ContainerAuto(ResponseShellContainerAuto),
    Local(ResponseLocalEnvironment),
    ContainerReference(ResponseContainerReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseShellContainerAuto {
    #[serde(rename = "type")]
    pub type_: ResponseShellContainerAutoType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_limit: Option<ResponseContainerMemoryLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<ResponseContainerNetworkPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<ResponseShellSkill>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseShellContainerAutoType {
    #[serde(rename = "container_auto")]
    ContainerAuto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseShellSkill {
    Reference(ResponseSkillReference),
    Inline(ResponseInlineSkill),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseSkillReference {
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseSkillReferenceType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseSkillReferenceType {
    #[serde(rename = "skill_reference")]
    SkillReference,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInlineSkill {
    pub description: String,
    pub name: String,
    pub source: ResponseInlineSkillSource,
    #[serde(rename = "type")]
    pub type_: ResponseInlineSkillType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseInlineSkillSource {
    pub data: String,
    pub media_type: ResponseInlineSkillMediaType,
    #[serde(rename = "type")]
    pub type_: ResponseInlineSkillSourceType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInlineSkillMediaType {
    #[serde(rename = "application/zip")]
    ApplicationZip,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInlineSkillSourceType {
    #[serde(rename = "base64")]
    Base64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseInlineSkillType {
    #[serde(rename = "inline")]
    Inline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCustomTool {
    pub name: String,
    #[serde(rename = "type")]
    pub type_: ResponseCustomToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<ResponseCustomToolInputFormat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolType {
    #[serde(rename = "custom")]
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseCustomToolInputFormat {
    Text(ResponseCustomToolTextFormat),
    Grammar(ResponseCustomToolGrammarFormat),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseCustomToolTextFormat {
    #[serde(rename = "type")]
    pub type_: ResponseCustomToolTextFormatType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolTextFormatType {
    #[serde(rename = "text")]
    Text,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseCustomToolGrammarFormat {
    pub definition: String,
    pub syntax: ResponseCustomToolGrammarSyntax,
    #[serde(rename = "type")]
    pub type_: ResponseCustomToolGrammarFormatType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolGrammarSyntax {
    #[serde(rename = "lark")]
    Lark,
    #[serde(rename = "regex")]
    Regex,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseCustomToolGrammarFormatType {
    #[serde(rename = "grammar")]
    Grammar,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebSearchPreviewTool {
    #[serde(rename = "type")]
    pub type_: ResponseWebSearchPreviewToolType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_content_types: Option<Vec<ResponseWebSearchPreviewContentType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<ResponseWebSearchContextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<ResponseWebSearchPreviewUserLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseWebSearchPreviewContentType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "image")]
    Image,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseWebSearchPreviewToolType {
    #[serde(rename = "web_search_preview")]
    WebSearchPreview,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebSearchPreviewUserLocation {
    #[serde(rename = "type")]
    pub type_: ResponseApproximateLocationType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseApplyPatchTool {
    #[serde(rename = "type")]
    pub type_: ResponseApplyPatchToolType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseApplyPatchToolType {
    #[serde(rename = "apply_patch")]
    ApplyPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResponseTruncation {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "disabled")]
    Disabled,
}
