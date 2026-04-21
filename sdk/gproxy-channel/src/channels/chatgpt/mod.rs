//! ChatGPT web channel (chatgpt.com `/backend-api/f/conversation` reverse).
//!
//! Status: algorithmic primitives only. Not yet wired into the `Channel`
//! registry — session, sentinel two-step, and the /f/conversation SSE v1
//! parser are still to come.

pub mod pow;
pub mod prepare_p;
pub mod sentinel;
pub mod session;
pub mod sse_to_openai;
pub mod sse_v1;
