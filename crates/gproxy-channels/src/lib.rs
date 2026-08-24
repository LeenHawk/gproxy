//! Built-in provider channel adapters.

mod aistudio;
mod aws_bedrock;
mod azure;
mod claudeapi;
mod claudecode;
mod codex;
mod custom;
mod openai;
mod openrouter;
mod shared;
mod vertex;
mod vertexexpress;

pub use aistudio::AiStudioChannel;
pub use aws_bedrock::AwsBedrockChannel;
pub use azure::AzureChannel;
pub use claudeapi::ClaudeApiChannel;
pub use claudecode::ClaudeCodeChannel;
pub use codex::CodexChannel;
pub use custom::CustomChannel;
pub use openai::OpenAiChannel;
pub use openrouter::OpenRouterChannel;
pub use vertex::VertexChannel;
pub use vertexexpress::VertexExpressChannel;
