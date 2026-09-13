//! Direct content-generation pairs. Each module owns its vendor-specific wire
//! mappings rather than normalizing content through an intermediate dialect.

pub mod chat_responses;
pub mod claude_chat;
pub mod claude_gemini;
pub mod claude_responses;
pub mod gemini_chat;
pub mod gemini_responses;
pub mod gemini_schema;
