//! Direct content-generation pairs. Each module owns its vendor-specific wire
//! mappings rather than normalizing content through an intermediate dialect.

pub mod chat_responses;
pub mod claude_chat;
pub(crate) mod claude_controls;
pub mod claude_gemini;
pub mod claude_responses;
pub(crate) mod client_tools;
pub mod gemini_chat;
pub mod gemini_responses;
pub mod gemini_schema;
pub(crate) mod openai_controls;

pub(crate) mod reasoning_details;
pub mod stream;
