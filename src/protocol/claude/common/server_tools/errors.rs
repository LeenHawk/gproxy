use serde::{Deserialize, Serialize};

use super::super::JsonObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerToolResultError<C, T> {
    pub error_code: C,
    #[serde(rename = "type")]
    pub type_: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "JsonObject::is_empty")]
    pub extra: JsonObject,
}

macro_rules! error_code {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(untagged)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $known {
            $(#[serde(rename = $wire)] $variant,)+
        }
    };
}

macro_rules! error_type {
    ($name:ident { $variant:ident => $wire:literal }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}

error_code!(WebSearchToolResultErrorCode, WebSearchToolResultErrorCodeKnown {
    InvalidToolInput => "invalid_tool_input",
    Unavailable => "unavailable",
    MaxUsesExceeded => "max_uses_exceeded",
    TooManyRequests => "too_many_requests",
    QueryTooLong => "query_too_long",
    RequestTooLarge => "request_too_large",
});
error_type!(WebSearchToolResultErrorType {
    WebSearchToolResultError => "web_search_tool_result_error"
});
pub type WebSearchToolResultError =
    ServerToolResultError<WebSearchToolResultErrorCode, WebSearchToolResultErrorType>;

error_code!(WebFetchToolResultErrorCode, WebFetchToolResultErrorCodeKnown {
    InvalidToolInput => "invalid_tool_input",
    UrlTooLong => "url_too_long",
    UrlNotAllowed => "url_not_allowed",
    UrlNotInPriorContext => "url_not_in_prior_context",
    UrlNotAccessible => "url_not_accessible",
    UnsupportedContentType => "unsupported_content_type",
    TooManyRequests => "too_many_requests",
    MaxUsesExceeded => "max_uses_exceeded",
    Unavailable => "unavailable",
});
error_type!(WebFetchToolResultErrorType {
    WebFetchToolResultError => "web_fetch_tool_result_error"
});
pub type WebFetchToolResultError =
    ServerToolResultError<WebFetchToolResultErrorCode, WebFetchToolResultErrorType>;

error_code!(AdvisorToolResultErrorCode, AdvisorToolResultErrorCodeKnown {
    MaxUsesExceeded => "max_uses_exceeded",
    PromptTooLong => "prompt_too_long",
    TooManyRequests => "too_many_requests",
    Overloaded => "overloaded",
    Unavailable => "unavailable",
    ExecutionTimeExceeded => "execution_time_exceeded",
});
error_type!(AdvisorToolResultErrorType {
    AdvisorToolResultError => "advisor_tool_result_error"
});
pub type AdvisorToolResultError =
    ServerToolResultError<AdvisorToolResultErrorCode, AdvisorToolResultErrorType>;

error_code!(CodeExecutionToolResultErrorCode, CodeExecutionToolResultErrorCodeKnown {
    InvalidToolInput => "invalid_tool_input",
    Unavailable => "unavailable",
    TooManyRequests => "too_many_requests",
    ExecutionTimeExceeded => "execution_time_exceeded",
});
error_type!(CodeExecutionToolResultErrorType {
    CodeExecutionToolResultError => "code_execution_tool_result_error"
});
pub type CodeExecutionToolResultError =
    ServerToolResultError<CodeExecutionToolResultErrorCode, CodeExecutionToolResultErrorType>;

error_code!(
    BashCodeExecutionToolResultErrorCode,
    BashCodeExecutionToolResultErrorCodeKnown {
        InvalidToolInput => "invalid_tool_input",
        Unavailable => "unavailable",
        TooManyRequests => "too_many_requests",
        ExecutionTimeExceeded => "execution_time_exceeded",
        OutputFileTooLarge => "output_file_too_large",
    }
);
error_type!(BashCodeExecutionToolResultErrorType {
    BashCodeExecutionToolResultError => "bash_code_execution_tool_result_error"
});
pub type BashCodeExecutionToolResultError = ServerToolResultError<
    BashCodeExecutionToolResultErrorCode,
    BashCodeExecutionToolResultErrorType,
>;

error_code!(
    TextEditorCodeExecutionToolResultErrorCode,
    TextEditorCodeExecutionToolResultErrorCodeKnown {
        InvalidToolInput => "invalid_tool_input",
        Unavailable => "unavailable",
        TooManyRequests => "too_many_requests",
        ExecutionTimeExceeded => "execution_time_exceeded",
        FileNotFound => "file_not_found",
    }
);
error_type!(TextEditorCodeExecutionToolResultErrorType {
    TextEditorCodeExecutionToolResultError => "text_editor_code_execution_tool_result_error"
});
pub type TextEditorCodeExecutionToolResultError = ServerToolResultError<
    TextEditorCodeExecutionToolResultErrorCode,
    TextEditorCodeExecutionToolResultErrorType,
>;

error_code!(ToolSearchToolResultErrorCode, ToolSearchToolResultErrorCodeKnown {
    InvalidToolInput => "invalid_tool_input",
    Unavailable => "unavailable",
    TooManyRequests => "too_many_requests",
    ExecutionTimeExceeded => "execution_time_exceeded",
});
error_type!(ToolSearchToolResultErrorType {
    ToolSearchToolResultError => "tool_search_tool_result_error"
});
pub type ToolSearchToolResultError =
    ServerToolResultError<ToolSearchToolResultErrorCode, ToolSearchToolResultErrorType>;
