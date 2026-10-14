//! Typed model resource conversions.
//!
//! Model items and list pagination are intentionally separate. A model item
//! can be converted without pretending that a cursor from one vendor is a
//! page token in another vendor.

mod common;
mod get;
mod list;
mod supplement;

pub use get::{
    claude_to_gemini, claude_to_openai, gemini_to_claude, gemini_to_openai, openai_to_claude,
    openai_to_gemini,
};
pub use list::{
    claude_to_gemini_list, claude_to_openai_list, gemini_to_claude_list, gemini_to_openai_list,
    normalize_openai_list, openai_to_claude_list, openai_to_gemini_list,
};
pub use supplement::{
    ClaudeCapabilities, ClaudeContextManagement, ClaudeEffort, ClaudeModelSupplement,
    ClaudeServerTools, ClaudeThinking, Converted, GeminiModelSupplement, ListPageFacts,
    OpenAiModelSupplement,
};
