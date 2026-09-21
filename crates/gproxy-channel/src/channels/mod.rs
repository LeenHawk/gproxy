//! Concrete channel implementations. None is enabled by default; the host
//! registers the ones it compiled in.

#[cfg(feature = "aistudio")]
pub mod aistudio;
#[cfg(feature = "antigravity")]
pub mod antigravity;
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
#[cfg(feature = "dashscope")]
pub mod dashscope;
#[cfg(feature = "deepseek")]
pub mod deepseek;
#[cfg(feature = "devin")]
pub mod devin;
#[cfg(feature = "geminicli")]
pub mod geminicli;
#[cfg(feature = "kimi")]
pub mod kimi;
#[cfg(feature = "openai")]
pub mod openai;
#[cfg(feature = "openrouter")]
pub mod openrouter;
#[cfg(any(
    feature = "aistudio",
    feature = "antigravity",
    feature = "azure",
    feature = "claudeapi",
    feature = "codex",
    feature = "claudecode",
    feature = "claudeweb",
    feature = "custom",
    feature = "geminicli",
    feature = "dashscope",
    feature = "deepseek",
    feature = "kimi",
    feature = "openai",
    feature = "openrouter",
    feature = "vertex",
    feature = "vertexexpress",
    feature = "xai"
))]
mod shared;
#[cfg(feature = "vertex")]
pub mod vertex;
#[cfg(feature = "vertexexpress")]
pub mod vertexexpress;
#[cfg(feature = "xai")]
pub mod xai;
