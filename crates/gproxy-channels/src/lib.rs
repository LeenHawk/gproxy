//! Built-in provider channel adapters.

mod aistudio;
mod antigravity;
mod aws_bedrock;
mod azure;
mod claudeapi;
mod claudecode;
#[cfg(not(target_arch = "wasm32"))]
mod claudeweb;
mod cline;
mod cloudflare_ai_gateway;
mod codex;
mod copilotcli;
mod custom;
mod dashscope;
mod deepseek;
mod geminicli;
mod grokbuild;
mod groq;
mod kimi;
mod kiro;
mod legacy;
mod metadata;
mod nvidia;
mod openai;
mod opencode;
mod openrouter;
mod policy;
mod shared;
mod vercel;
mod vertex;
mod vertexexpress;
mod workbuddy;
mod xai;

pub use aistudio::AiStudioChannel;
pub use antigravity::AntigravityChannel;
pub use aws_bedrock::AwsBedrockChannel;
pub use azure::AzureChannel;
pub use claudeapi::ClaudeApiChannel;
pub use claudecode::ClaudeCodeChannel;
#[cfg(not(target_arch = "wasm32"))]
pub use claudeweb::ClaudeWebChannel;
pub use cline::ClineChannel;
pub use cloudflare_ai_gateway::CloudflareAiGatewayChannel;
pub use codex::CodexChannel;
pub use copilotcli::CopilotCliChannel;
pub use custom::CustomChannel;
pub use dashscope::DashScopeChannel;
pub use deepseek::DeepSeekChannel;
pub use geminicli::GeminiCliChannel;
pub use grokbuild::GrokBuildChannel;
pub use groq::GroqChannel;
pub use kimi::KimiChannel;
pub use kiro::KiroChannel;
pub use legacy::provider_settings as canonical_provider_settings;
pub use nvidia::NvidiaChannel;
pub use openai::OpenAiChannel;
pub use opencode::OpenCodeChannel;
pub use openrouter::OpenRouterChannel;
pub use vercel::VercelChannel;
pub use vertex::VertexChannel;
pub use vertexexpress::VertexExpressChannel;
pub use workbuddy::WorkBuddyChannel;
pub use xai::XaiChannel;

/// Apply the frozen client-to-proxy Claude cache marker protocol.
pub fn apply_claude_magic_cache(
    body: bytes::Bytes,
) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
    let mut value = serde_json::from_slice(&body).map_err(|error| {
        gproxy_channel_api::ChannelError::Prepare(format!("Claude request JSON: {error}"))
    })?;
    shared::cache::claude(&mut value);
    serde_json::to_vec(&value)
        .map(bytes::Bytes::from)
        .map_err(|error| gproxy_channel_api::ChannelError::Prepare(error.to_string()))
}

/// Canonicalize channel ids at the legacy configuration import boundary.
pub fn canonical_channel_id(id: &str) -> &str {
    match id {
        "kimiapi" | "kimicode" => "kimi",
        "opencodezen" | "opencodego" => "opencode",
        _ => id,
    }
}

#[cfg(test)]
mod compatibility_tests;
#[cfg(test)]
mod login_tests;
