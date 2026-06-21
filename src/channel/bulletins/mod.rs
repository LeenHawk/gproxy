//! Channel adapters ("bulletins") — one folder per channel; each manages its
//! own auth in `auth.rs`. API-key channels share [`common`]; OAuth/envelope
//! channels carry their own OAuth refresh + transform (M7 / M2, both landed).

pub mod common;

#[cfg(test)]
mod tests;

// API-key channels (functional)
pub mod aistudio;
pub mod claudeapi;
pub mod custom;
pub mod deepseek;
pub mod groq;
pub mod nvidia;
pub mod openai;
pub mod openrouter;
pub mod vercel;
pub mod vertexexpress;

// OAuth / envelope channels (functional)
pub mod antigravity;
pub mod claudecode;
pub mod codex;
pub mod copilotcli;
pub mod geminicli;
pub mod kiro;
pub mod vertex;
