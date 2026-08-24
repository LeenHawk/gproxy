//! Built-in provider channel adapters.

mod aistudio;
mod claudeapi;
mod claudecode;
mod codex;
mod openai;
mod shared;

pub use aistudio::AiStudioChannel;
pub use claudeapi::ClaudeApiChannel;
pub use claudecode::ClaudeCodeChannel;
pub use codex::CodexChannel;
pub use openai::OpenAiChannel;
