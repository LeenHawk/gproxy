//! Buffered OpenAI Chat Completions ↔ Responses conversion.
//!
//! The pair is intentionally direct and field based. Message history is
//! reconstructed into the target protocol's declared input/output items; no
//! common content representation or source `rest` map is used.

pub(crate) use super::client_tools;
mod common;
mod request;
mod response;

pub use request::{
    ToolCallKind, chat_to_responses_request, chat_to_responses_request_with_calls,
    responses_to_chat_request,
};
pub use response::{
    ChatUsageSupplement, ResponsesResponseContext, chat_to_responses_response,
    responses_to_chat_response,
};

pub mod stream;
