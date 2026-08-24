//! Built-in provider channel adapters.

mod aistudio;
mod aws_bedrock;
mod azure;
mod claudeapi;
mod claudecode;
mod cloudflare_ai_gateway;
mod codex;
mod custom;
mod dashscope;
mod deepseek;
mod groq;
mod kimi;
mod nvidia;
mod openai;
mod openrouter;
mod shared;
mod vercel;
mod vertex;
mod vertexexpress;
mod xai;

pub use aistudio::AiStudioChannel;
pub use aws_bedrock::AwsBedrockChannel;
pub use azure::AzureChannel;
pub use claudeapi::ClaudeApiChannel;
pub use claudecode::ClaudeCodeChannel;
pub use cloudflare_ai_gateway::CloudflareAiGatewayChannel;
pub use codex::CodexChannel;
pub use custom::CustomChannel;
pub use dashscope::DashScopeChannel;
pub use deepseek::DeepSeekChannel;
pub use groq::GroqChannel;
pub use kimi::KimiChannel;
pub use nvidia::NvidiaChannel;
pub use openai::OpenAiChannel;
pub use openrouter::OpenRouterChannel;
pub use vercel::VercelChannel;
pub use vertex::VertexChannel;
pub use vertexexpress::VertexExpressChannel;
pub use xai::XaiChannel;

/// Canonicalize channel ids at the legacy configuration import boundary.
pub fn canonical_channel_id(id: &str) -> &str {
    match id {
        "kimiapi" | "kimicode" => "kimi",
        _ => id,
    }
}
