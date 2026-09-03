use serde::{Deserialize, Serialize};

use super::super::{CacheControl, Caller, JsonObject, WebSearchResultBlock};
use super::{
    AdvisorRedactedResultBlock, AdvisorResultBlock, AdvisorToolResultError,
    BashCodeExecutionResultBlock, BashCodeExecutionToolResultError, CodeExecutionResultBlock,
    CodeExecutionToolResultError, EncryptedCodeExecutionResultBlock,
    TextEditorCodeExecutionCreateResultBlock, TextEditorCodeExecutionStrReplaceResultBlock,
    TextEditorCodeExecutionToolResultError, TextEditorCodeExecutionViewResultBlock,
    ToolSearchToolResultError, ToolSearchToolSearchResultBlock, WebFetchResultBlock,
    WebFetchToolResultError, WebSearchToolResultError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum WebSearchToolResultContent {
    Error(WebSearchToolResultError),
    Results(Vec<WebSearchResultBlock>),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum WebFetchToolResultContent {
    Error(WebFetchToolResultError),
    Result(Box<WebFetchResultBlock>),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum AdvisorToolResultContent {
    Error(AdvisorToolResultError),
    Result(AdvisorResultBlock),
    Redacted(AdvisorRedactedResultBlock),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CodeExecutionToolResultContent {
    Error(CodeExecutionToolResultError),
    Result(CodeExecutionResultBlock),
    Encrypted(EncryptedCodeExecutionResultBlock),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum BashCodeExecutionToolResultContent {
    Error(BashCodeExecutionToolResultError),
    Result(BashCodeExecutionResultBlock),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum TextEditorCodeExecutionToolResultContent {
    Error(TextEditorCodeExecutionToolResultError),
    View(TextEditorCodeExecutionViewResultBlock),
    Create(TextEditorCodeExecutionCreateResultBlock),
    StrReplace(TextEditorCodeExecutionStrReplaceResultBlock),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ToolSearchToolResultContent {
    Error(ToolSearchToolResultError),
    Result(ToolSearchToolSearchResultBlock),
    Raw(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct WebFetchToolResultBlock {
    pub content: WebFetchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: WebFetchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum WebFetchToolResultBlockType {
    #[serde(rename = "web_fetch_tool_result")]
    WebFetchToolResult,
}

macro_rules! server_tool_result_block {
    ($block:ident, $content:ident, $tag:ident, $wire:literal, $variant:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $block {
            pub content: $content,
            pub tool_use_id: String,
            #[serde(rename = "type")]
            pub type_: $tag,
            #[serde(skip_serializing_if = "Option::is_none")]
            pub cache_control: Option<CacheControl>,
            #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
            pub rest: serde_json::Map<String, serde_json::Value>,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum $tag {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

server_tool_result_block!(
    AdvisorToolResultBlock,
    AdvisorToolResultContent,
    AdvisorToolResultBlockType,
    "advisor_tool_result",
    AdvisorToolResult
);
server_tool_result_block!(
    CodeExecutionToolResultBlock,
    CodeExecutionToolResultContent,
    CodeExecutionToolResultBlockType,
    "code_execution_tool_result",
    CodeExecutionToolResult
);
server_tool_result_block!(
    BashCodeExecutionToolResultBlock,
    BashCodeExecutionToolResultContent,
    BashCodeExecutionToolResultBlockType,
    "bash_code_execution_tool_result",
    BashCodeExecutionToolResult
);
server_tool_result_block!(
    TextEditorCodeExecutionToolResultBlock,
    TextEditorCodeExecutionToolResultContent,
    TextEditorCodeExecutionToolResultBlockType,
    "text_editor_code_execution_tool_result",
    TextEditorCodeExecutionToolResult
);
server_tool_result_block!(
    ToolSearchToolResultBlock,
    ToolSearchToolResultContent,
    ToolSearchToolResultBlockType,
    "tool_search_tool_result",
    ToolSearchToolResult
);
