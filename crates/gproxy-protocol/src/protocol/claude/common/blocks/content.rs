use serde::{Deserialize, Serialize};

use super::super::server_tools::*;
use super::super::{JsonObject, MessageRole, StringOrArray, TypedObject};
use super::*;

pub type MessageContent = StringOrArray<ContentBlockParam>;
pub type SystemPrompt = StringOrArray<TextBlock>;
pub type ContentBlock = ResponseContentBlock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageParam {
    pub role: MessageRole,
    pub content: MessageContent,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContentBlockParam {
    Text(TextBlock),
    Image(ImageBlock),
    Document(DocumentBlock),
    SearchResult(SearchResultBlock),
    Thinking(ThinkingBlock),
    RedactedThinking(RedactedThinkingBlock),
    ToolUse(ToolUseBlock),
    ToolResult(ToolResultBlock),
    ServerToolUse(ServerToolUseBlock),
    WebSearchToolResult(WebSearchToolResultBlock),
    WebFetchToolResult(WebFetchToolResultBlock),
    AdvisorToolResult(AdvisorToolResultBlock),
    CodeExecutionToolResult(CodeExecutionToolResultBlock),
    BashCodeExecutionToolResult(BashCodeExecutionToolResultBlock),
    TextEditorCodeExecutionToolResult(TextEditorCodeExecutionToolResultBlock),
    ToolSearchToolResult(ToolSearchToolResultBlock),
    McpToolUse(McpToolUseBlock),
    McpToolResult(McpToolResultBlock),
    ContainerUpload(ContainerUploadBlock),
    Compaction(CompactionBlock),
    MidConversationSystem(MidConversationSystemBlock),
    Fallback(FallbackBlock),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseContentBlock {
    Text(ResponseTextBlock),
    Thinking(ThinkingBlock),
    RedactedThinking(RedactedThinkingBlock),
    ToolUse(ResponseToolUseBlock),
    ServerToolUse(ResponseServerToolUseBlock),
    WebSearchToolResult(ResponseWebSearchToolResultBlock),
    WebFetchToolResult(ResponseWebFetchToolResultBlock),
    AdvisorToolResult(ResponseAdvisorToolResultBlock),
    CodeExecutionToolResult(ResponseCodeExecutionToolResultBlock),
    BashCodeExecutionToolResult(ResponseBashCodeExecutionToolResultBlock),
    TextEditorCodeExecutionToolResult(ResponseTextEditorCodeExecutionToolResultBlock),
    ToolSearchToolResult(ResponseToolSearchToolResultBlock),
    McpToolUse(ResponseMcpToolUseBlock),
    McpToolResult(ResponseMcpToolResultBlock),
    ContainerUpload(ResponseContainerUploadBlock),
    Compaction(ResponseCompactionBlock),
    Fallback(FallbackBlock),
    Raw(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultContentBlock>),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContentBlock {
    Text(TextBlock),
    Image(ImageBlock),
    SearchResult(SearchResultBlock),
    Document(DocumentBlock),
    ToolReference(ToolReferenceBlock),
    Raw(TypedObject),
}

pub type McpToolResultContent = StringOrArray<TextBlock>;
