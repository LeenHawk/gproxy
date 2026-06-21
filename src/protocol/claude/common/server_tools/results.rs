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
pub enum WebSearchToolResultContent {
    Error(WebSearchToolResultError),
    Results(Vec<WebSearchResultBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WebFetchToolResultContent {
    Error(WebFetchToolResultError),
    Result(WebFetchResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdvisorToolResultContent {
    Error(AdvisorToolResultError),
    Result(AdvisorResultBlock),
    Redacted(AdvisorRedactedResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CodeExecutionToolResultContent {
    Error(CodeExecutionToolResultError),
    Result(CodeExecutionResultBlock),
    Encrypted(EncryptedCodeExecutionResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BashCodeExecutionToolResultContent {
    Error(BashCodeExecutionToolResultError),
    Result(BashCodeExecutionResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TextEditorCodeExecutionToolResultContent {
    Error(TextEditorCodeExecutionToolResultError),
    View(TextEditorCodeExecutionViewResultBlock),
    Create(TextEditorCodeExecutionCreateResultBlock),
    StrReplace(TextEditorCodeExecutionStrReplaceResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolSearchToolResultContent {
    Error(ToolSearchToolResultError),
    Result(ToolSearchToolSearchResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            pub extra: JsonObject,
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
