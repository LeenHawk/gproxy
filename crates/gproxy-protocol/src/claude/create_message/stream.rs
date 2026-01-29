use serde::{Deserialize, Serialize};

use crate::claude::count_tokens::types::Model;
use crate::claude::create_message::types::{
    BetaBashCodeExecutionToolResultBlock, BetaCacheCreation, BetaCodeExecutionToolResultBlock,
    BetaContainer, BetaContainerUploadBlock, BetaContextManagementResponse, BetaMcpToolResultBlock,
    BetaMcpToolUseBlock, BetaMessageRole, BetaMessageType, BetaRedactedThinkingBlock,
    BetaServerToolName, BetaServerToolUsage, BetaServerToolUseBlockType, BetaStopReason,
    BetaTextBlock, BetaTextEditorCodeExecutionToolResultBlock, BetaThinkingBlockType,
    BetaToolCaller, BetaToolSearchToolResultBlock, BetaToolUseBlock, BetaWebFetchToolResultBlock,
    BetaWebSearchToolResultBlock, JsonObject, JsonValue,
};
use crate::claude::error::ErrorDetail;
use crate::claude::types::RequestId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaStreamUsage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation: Option<BetaCacheCreation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_tool_use: Option<BetaServerToolUsage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaStreamMessage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<BetaContainer>,
    /// Message start events include an empty content array.
    pub content: Vec<BetaStreamContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_management: Option<BetaContextManagementResponse>,
    pub model: Model,
    pub role: BetaMessageRole,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(rename = "type")]
    pub r#type: BetaMessageType,
    pub usage: BetaStreamUsage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaStreamMessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<BetaStopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaThinkingBlockStream {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    pub thinking: String,
    #[serde(rename = "type")]
    pub r#type: BetaThinkingBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BetaServerToolUseBlockStream {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<BetaToolCaller>,
    pub input: JsonObject,
    pub name: BetaServerToolName,
    #[serde(rename = "type")]
    pub r#type: BetaServerToolUseBlockType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BetaStreamContentBlock {
    Text(BetaTextBlock),
    Thinking(BetaThinkingBlockStream),
    RedactedThinking(BetaRedactedThinkingBlock),
    ToolUse(BetaToolUseBlock),
    ServerToolUse(BetaServerToolUseBlockStream),
    WebSearchToolResult(BetaWebSearchToolResultBlock),
    WebFetchToolResult(BetaWebFetchToolResultBlock),
    CodeExecutionToolResult(BetaCodeExecutionToolResultBlock),
    BashCodeExecutionToolResult(BetaBashCodeExecutionToolResultBlock),
    TextEditorCodeExecutionToolResult(BetaTextEditorCodeExecutionToolResultBlock),
    ToolSearchToolResult(BetaToolSearchToolResultBlock),
    McpToolUse(BetaMcpToolUseBlock),
    McpToolResult(BetaMcpToolResultBlock),
    ContainerUpload(BetaContainerUploadBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaStreamContentBlockDelta {
    TextDelta {
        text: String,
    },
    /// Partial JSON string; accumulate and parse after content_block_stop.
    InputJsonDelta {
        partial_json: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    SignatureDelta {
        signature: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BetaStreamEventKnown {
    MessageStart {
        message: BetaStreamMessage,
    },
    ContentBlockStart {
        index: u32,
        content_block: BetaStreamContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: BetaStreamContentBlockDelta,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: BetaStreamMessageDelta,
        /// Token counts are cumulative for the stream so far.
        usage: BetaStreamUsage,
    },
    MessageStop,
    Ping,
    Error {
        error: ErrorDetail,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<RequestId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum BetaStreamEvent {
    Known(BetaStreamEventKnown),
    Unknown(JsonValue),
}
