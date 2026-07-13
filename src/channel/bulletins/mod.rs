//! Channel adapters ("bulletins") — one folder per channel; each manages its
//! own auth in `auth.rs`. API-key channels share [`common`]; OAuth/envelope
//! channels carry their own OAuth refresh + transform (M7 / M2, both landed).

pub mod common;

#[cfg(test)]
mod tests;

// One feature per bulletin (all default-on; the browser-session web channels
// are native-only and excluded from the edge/wasm subset). The registry gates
// each entry to match.
#[cfg(feature = "channel-aistudio")]
pub mod aistudio;
#[cfg(feature = "channel-claudeapi")]
pub mod claudeapi;
#[cfg(feature = "channel-custom")]
pub mod custom;
#[cfg(feature = "channel-deepseek")]
pub mod deepseek;
#[cfg(feature = "channel-groq")]
pub mod groq;
#[cfg(feature = "channel-nvidia")]
pub mod nvidia;
#[cfg(feature = "channel-openai")]
pub mod openai;
#[cfg(feature = "channel-openrouter")]
pub mod openrouter;
#[cfg(feature = "channel-vercel")]
pub mod vercel;
#[cfg(feature = "channel-vertexexpress")]
pub mod vertexexpress;

// OAuth / envelope channels (functional)
#[cfg(feature = "channel-antigravity")]
pub mod antigravity;
#[cfg(feature = "channel-chatgpt")]
pub mod chatgpt;
#[cfg(feature = "channel-claudecode")]
pub mod claudecode;
#[cfg(all(feature = "channel-claudeweb", not(target_arch = "wasm32")))]
pub mod claudeweb;
#[cfg(feature = "channel-codex")]
pub mod codex;
#[cfg(feature = "channel-copilotcli")]
pub mod copilotcli;
#[cfg(feature = "channel-geminicli")]
pub mod geminicli;
#[cfg(feature = "channel-grokbuild")]
pub mod grokbuild;
#[cfg(feature = "channel-kiro")]
pub mod kiro;
#[cfg(feature = "channel-vertex")]
pub mod vertex;
