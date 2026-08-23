//! Built-in provider channel adapters.

mod aistudio;
mod claudecode;
mod codex;
mod openai;

pub use aistudio::AiStudioChannel;
pub use claudecode::ClaudeCodeChannel;
pub use codex::CodexChannel;
pub use openai::OpenAiChannel;
