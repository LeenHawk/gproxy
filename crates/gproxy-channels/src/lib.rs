//! Built-in provider channel adapters.

mod claudecode;
mod codex;
mod openai;

pub use claudecode::ClaudeCodeChannel;
pub use codex::CodexChannel;
pub use openai::OpenAiChannel;
