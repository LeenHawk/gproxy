//! Built-in provider channel adapters.

mod claudecode;
mod openai;

pub use claudecode::ClaudeCodeChannel;
pub use openai::OpenAiChannel;
