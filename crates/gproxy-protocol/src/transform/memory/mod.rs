//! Typed memory-summary tasks. Trace items are formal arbitrary JSON data.

mod request;
mod response;
pub use request::{
    MemoryDialectRequest, build_claude, build_gemini, build_openai_chat, build_openai_responses,
    trace_payload,
};
pub use response::{chat_text, claude_text, gemini_text, parse_output, responses_text};
