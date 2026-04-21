//! ChatGPT web channel (chatgpt.com `/backend-api/f/conversation` reverse).
//!
//! The Channel trait implementation lives in [`channel`]; the other modules
//! hold reverse-engineered sentinel primitives, the SSE v1 decoder, the
//! OpenAI-chunk converter, and request/body builders.
//!
//! # Transport
//!
//! Cloudflare in front of chatgpt.com validates both TLS/H2 fingerprint AND
//! a `__cf_bm` cookie. The channel therefore opts into
//! [`Channel::needs_spoof_client`] so the engine always uses its
//! browser-impersonating `spoof_client`. That client is built with
//! `cookie_store(true)` in `gproxy_engine::engine` so the `__cf_bm` cookie
//! issued during the warmup call in [`Channel::refresh_credential`]
//! persists into subsequent `/f/conversation` calls automatically.

pub mod channel;
pub mod image;
pub mod image_edit;
pub mod pow;
pub mod prepare_p;
pub mod request_builder;
pub mod sentinel;
pub mod session;
pub mod sse_to_openai;
pub mod sse_v1;

pub use channel::{ChatGptChannel, ChatGptCredential, ChatGptSettings};
