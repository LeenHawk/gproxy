//! Direct Claude Messages ↔ OpenAI Responses buffered conversion.

pub(crate) mod request;
mod response;
pub mod stream;

pub use request::{ClaudeRequestContext, claude_to_responses_request, responses_to_claude_request};
pub use response::{
    ClaudeResponseContext, ResponsesUsageFacts, claude_to_responses_response,
    responses_to_claude_response,
};
