//! File metadata mapping and transfer boundaries.
//!
//! This module intentionally separates typed wire mapping from transfer. A
//! metadata conversion cannot download, upload, delete, or invent provider
//! timestamps. Actual transfers are composed by adapt::files over host capabilities.

mod mapping;

pub use mapping::{
    FileFacts, FilePurpose, FileStatusFacts, claude_to_gemini, claude_to_openai, gemini_to_claude,
    gemini_to_openai, openai_to_claude, openai_to_gemini,
};
