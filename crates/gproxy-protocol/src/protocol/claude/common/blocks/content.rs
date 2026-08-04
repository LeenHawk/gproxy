use serde::{Deserialize, Serialize};

use super::super::server_tools::*;
use super::super::{JsonObject, MessageRole, StringOrArray, TypedObject};
use super::*;
use crate::protocol::extensible::type_tag_union_deserialize;

pub type MessageContent = StringOrArray<ContentBlockParam>;
pub type SystemPrompt = StringOrArray<TextBlock>;
pub type ContentBlock = ResponseContentBlock;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct MessageParam {
    pub role: MessageRole,
    pub content: MessageContent,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

// Deserialize is hand-rolled below: one `type` tag match instead of untagged
// trials. Serialization stays derive(untagged) — each block struct carries its
// own `type` witness field.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
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
    ToolAddition(ToolAdditionBlock),
    ToolRemoval(ToolRemovalBlock),
    Fallback(FallbackBlock),
    Raw(TypedObject),
}

type_tag_union_deserialize!(ContentBlockParam {
    "text" => Text,
    "image" => Image,
    "document" => Document,
    "search_result" => SearchResult,
    "thinking" => Thinking,
    "redacted_thinking" => RedactedThinking,
    "tool_use" => ToolUse,
    "tool_result" => ToolResult,
    "server_tool_use" => ServerToolUse,
    "web_search_tool_result" => WebSearchToolResult,
    "web_fetch_tool_result" => WebFetchToolResult,
    "advisor_tool_result" => AdvisorToolResult,
    "code_execution_tool_result" => CodeExecutionToolResult,
    "bash_code_execution_tool_result" => BashCodeExecutionToolResult,
    "text_editor_code_execution_tool_result" => TextEditorCodeExecutionToolResult,
    "tool_search_tool_result" => ToolSearchToolResult,
    "mcp_tool_use" => McpToolUse,
    "mcp_tool_result" => McpToolResult,
    "container_upload" => ContainerUpload,
    "compaction" => Compaction,
    "mid_conv_system" => MidConversationSystem,
    "tool_addition" => ToolAddition,
    "tool_removal" => ToolRemoval,
    "fallback" => Fallback,
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
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

type_tag_union_deserialize!(ResponseContentBlock {
    "text" => Text,
    "thinking" => Thinking,
    "redacted_thinking" => RedactedThinking,
    "tool_use" => ToolUse,
    "server_tool_use" => ServerToolUse,
    "web_search_tool_result" => WebSearchToolResult,
    "web_fetch_tool_result" => WebFetchToolResult,
    "advisor_tool_result" => AdvisorToolResult,
    "code_execution_tool_result" => CodeExecutionToolResult,
    "bash_code_execution_tool_result" => BashCodeExecutionToolResult,
    "text_editor_code_execution_tool_result" => TextEditorCodeExecutionToolResult,
    "tool_search_tool_result" => ToolSearchToolResult,
    "mcp_tool_use" => McpToolUse,
    "mcp_tool_result" => McpToolResult,
    "container_upload" => ContainerUpload,
    "compaction" => Compaction,
    "fallback" => Fallback,
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ToolResultContentBlock>),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ToolResultContentBlock {
    Text(TextBlock),
    Image(ImageBlock),
    SearchResult(SearchResultBlock),
    Document(DocumentBlock),
    ToolReference(ToolReferenceBlock),
    Raw(TypedObject),
}

type_tag_union_deserialize!(ToolResultContentBlock {
    "text" => Text,
    "image" => Image,
    "search_result" => SearchResult,
    "document" => Document,
    "tool_reference" => ToolReference,
});

pub type McpToolResultContent = StringOrArray<TextBlock>;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn tag_dispatch_falls_back_to_raw() {
        // Unknown tag round-trips unchanged through Raw.
        let wire = json!({"type": "hologram", "payload": {"x": 1}});
        let block: ContentBlockParam = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(block, ContentBlockParam::Raw(_)));
        assert_eq!(serde_json::to_value(&block).unwrap(), wire);

        // A known tag whose body no longer parses degrades to Raw and is
        // forwarded unchanged (pre-dispatch untagged behavior).
        let wire = json!({"type": "tool_use", "id": 42});
        let block: ContentBlockParam = serde_json::from_value(wire.clone()).unwrap();
        assert!(matches!(block, ContentBlockParam::Raw(_)));
        assert_eq!(serde_json::to_value(&block).unwrap(), wire);
    }
}
