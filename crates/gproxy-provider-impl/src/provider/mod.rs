pub mod aistudio;
pub mod antigravity;
pub mod claude;
pub mod claudecode;
pub mod codex;
pub mod deepseek;
pub mod geminicli;
pub mod nvidia;
pub mod openai;
pub mod vertex;
pub mod vertexexpress;

pub use aistudio::AistudioProvider;
pub use antigravity::AntiGravityProvider;
pub use claude::ClaudeProvider;
pub use claudecode::ClaudeCodeProvider;
pub use codex::CodexProvider;
pub use deepseek::DeepSeekProvider;
pub use geminicli::GeminiCliProvider;
pub use nvidia::NvidiaProvider;
pub use openai::OpenAIProvider;
pub use vertex::VertexProvider;
pub use vertexexpress::VertexExpressProvider;

use http::StatusCode;

use gproxy_provider_core::UpstreamPassthroughError;

pub(crate) fn not_implemented(provider: &str) -> UpstreamPassthroughError {
    UpstreamPassthroughError::from_status(
        StatusCode::NOT_IMPLEMENTED,
        format!("{provider} provider not implemented"),
    )
}
