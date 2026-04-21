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
//!
//! # Integration requirements
//!
//! This channel's upstream (`chatgpt.com`) sits behind a Cloudflare WAF that
//! issues `cf-mitigated: challenge` rejections unless the requesting
//! `wreq::Client` carries a `__cf_bm` cookie established by a prior origin
//! GET. [`session::warmup`] fires those GETs, but they only work if the
//! client was built with **`cookie_store(true)`**.
//!
//! The engine's default `http_client` / `spoof_client` does NOT enable
//! cookies, so hosts that wish to run this channel through
//! `GproxyEngine` must override the engine's client:
//!
//! ```no_run
//! use wreq::Client;
//! let client = Client::builder()
//!     .emulation(wreq_util::Emulation::Chrome136)
//!     .cookie_store(true)
//!     .build()
//!     .unwrap();
//! // `.http_client(client)` on the GproxyEngineBuilder.
//! ```
//!
//! Without this, `/f/conversation` calls will 403 with an
//! "Unusual activity" body even though the sentinel dance succeeds.

pub mod channel;
pub mod image;
pub mod pow;
pub mod prepare_p;
pub mod request_builder;
pub mod sentinel;
pub mod session;
pub mod sse_to_openai;
pub mod sse_v1;

pub use channel::{ChatGptChannel, ChatGptCredential, ChatGptSettings};
