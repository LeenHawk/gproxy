//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.

#[cfg(feature = "aistudio")]
pub mod aistudio;
#[cfg(feature = "aws_bedrock")]
pub mod aws_bedrock;
#[cfg(feature = "azure")]
pub mod azure;
#[cfg(feature = "claudeapi")]
pub mod claudeapi;
#[cfg(feature = "claudecode")]
pub mod claudecode;
#[cfg(feature = "claudeweb")]
pub mod claudeweb;
#[cfg(feature = "codex")]
pub mod codex;
#[cfg(feature = "custom")]
pub mod custom;
#[cfg(feature = "devin")]
pub mod devin;
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(any(
    feature = "aistudio",
    feature = "azure",
    feature = "claudeapi",
    feature = "codex",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "custom",
    feature = "openai",
    feature = "vertex",
    feature = "vertexexpress"
))]
mod shared;
#[cfg(feature = "vertex")]
pub mod vertex;
#[cfg(feature = "vertexexpress")]
pub mod vertexexpress;
