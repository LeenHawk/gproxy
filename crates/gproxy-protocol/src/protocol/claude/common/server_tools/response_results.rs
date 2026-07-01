use serde::{Deserialize, Serialize};

use super::super::{Caller, JsonObject, ResponseWebSearchResultBlock};
use super::{
    AdvisorToolResultBlockType, AdvisorToolResultError, BashCodeExecutionResultBlock,
    BashCodeExecutionToolResultBlockType, BashCodeExecutionToolResultError,
    CodeExecutionResultBlock, CodeExecutionToolResultBlockType, CodeExecutionToolResultError,
    EncryptedCodeExecutionResultBlock, ResponseAdvisorRedactedResultBlock,
    ResponseAdvisorResultBlock, ResponseTextEditorCodeExecutionStrReplaceResultBlock,
    ResponseTextEditorCodeExecutionToolResultError, ResponseTextEditorCodeExecutionViewResultBlock,
    ResponseToolSearchToolResultError, ResponseToolSearchToolSearchResultBlock,
    ResponseWebFetchResultBlock, TextEditorCodeExecutionCreateResultBlock,
    TextEditorCodeExecutionToolResultBlockType, ToolSearchToolResultBlockType,
    WebFetchToolResultBlockType, WebFetchToolResultError, WebSearchToolResultError,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseWebSearchToolResultContent {
    Error(WebSearchToolResultError),
    Results(Vec<ResponseWebSearchResultBlock>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseWebFetchToolResultContent {
    Error(WebFetchToolResultError),
    Result(ResponseWebFetchResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseAdvisorToolResultContent {
    Error(AdvisorToolResultError),
    Result(ResponseAdvisorResultBlock),
    Redacted(ResponseAdvisorRedactedResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseCodeExecutionToolResultContent {
    Error(CodeExecutionToolResultError),
    Result(CodeExecutionResultBlock),
    Encrypted(EncryptedCodeExecutionResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseBashCodeExecutionToolResultContent {
    Error(BashCodeExecutionToolResultError),
    Result(BashCodeExecutionResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseTextEditorCodeExecutionToolResultContent {
    Error(ResponseTextEditorCodeExecutionToolResultError),
    View(ResponseTextEditorCodeExecutionViewResultBlock),
    Create(TextEditorCodeExecutionCreateResultBlock),
    StrReplace(ResponseTextEditorCodeExecutionStrReplaceResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseToolSearchToolResultContent {
    Error(ResponseToolSearchToolResultError),
    Result(ResponseToolSearchToolSearchResultBlock),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseWebFetchToolResultBlock {
    pub content: ResponseWebFetchToolResultContent,
    pub tool_use_id: String,
    #[serde(rename = "type")]
    pub type_: WebFetchToolResultBlockType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller: Option<Caller>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

macro_rules! response_server_tool_result_block {
    ($block:ident, $content:ident, $tag:ident) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $block {
            pub content: $content,
            pub tool_use_id: String,
            #[serde(rename = "type")]
            pub type_: $tag,
            #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
            pub extra: JsonObject,
        }
    };
}

response_server_tool_result_block!(
    ResponseAdvisorToolResultBlock,
    ResponseAdvisorToolResultContent,
    AdvisorToolResultBlockType
);
response_server_tool_result_block!(
    ResponseCodeExecutionToolResultBlock,
    ResponseCodeExecutionToolResultContent,
    CodeExecutionToolResultBlockType
);
response_server_tool_result_block!(
    ResponseBashCodeExecutionToolResultBlock,
    ResponseBashCodeExecutionToolResultContent,
    BashCodeExecutionToolResultBlockType
);
response_server_tool_result_block!(
    ResponseTextEditorCodeExecutionToolResultBlock,
    ResponseTextEditorCodeExecutionToolResultContent,
    TextEditorCodeExecutionToolResultBlockType
);
response_server_tool_result_block!(
    ResponseToolSearchToolResultBlock,
    ResponseToolSearchToolResultContent,
    ToolSearchToolResultBlockType
);
