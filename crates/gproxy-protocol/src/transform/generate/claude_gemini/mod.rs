//! Buffered direct Claude Messages ↔ Gemini GenerateContent conversion.

mod config;
pub(crate) mod history;
pub(crate) mod media;
mod request;
mod response;
mod results;
mod schema;
pub mod stream;
pub(crate) mod thinking;
pub(crate) mod tools;
mod usage;

pub use media::MediaFacts;
pub use request::{ClaudeGeminiRequestContext, claude_to_gemini_request, gemini_to_claude_request};
pub use response::{claude_to_gemini_response, gemini_to_claude_response};
pub use thinking::{THINKING_HANDLE_PREFIX, is_thinking_handle, thinking_handle_id};
pub use usage::ClaudeGeminiUsageFacts;
