//! Direct Claude Messages ↔ OpenAI Responses buffered conversion.

mod request;
mod response;

pub use request::{
    ClaudeRequestContext, RestoredClaudeThinking, claude_to_responses_request,
    responses_to_claude_request,
};
pub use response::{
    ClaudeResponseContext, ResponsesUsageFacts, claude_to_responses_response,
    responses_to_claude_response, responses_to_claude_response_with_context,
};
