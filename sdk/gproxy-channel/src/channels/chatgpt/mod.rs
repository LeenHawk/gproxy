//! ChatGPT web channel (chatgpt.com `/backend-api/f/conversation` reverse).
//!
//! Status: algorithmic primitives only. Not yet wired into the `Channel`
//! registry — session, sentinel two-step, and the /f/conversation SSE v1
//! parser are still to come.

//! ChatGPT web channel (chatgpt.com `/backend-api/f/conversation` reverse).
//!
//! The Channel trait implementation lives in [`channel`]; the other modules
//! hold reverse-engineered sentinel primitives, the SSE v1 decoder, the
//! OpenAI-chunk converter, and request/body builders.

pub mod channel;
pub mod pow;
pub mod prepare_p;
pub mod request_builder;
pub mod sentinel;
pub mod session;
pub mod sse_to_openai;
pub mod sse_v1;

pub use channel::{ChatGptChannel, ChatGptCredential, ChatGptSettings};
