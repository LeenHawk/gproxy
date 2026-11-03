#![cfg(all(feature = "realtime", not(target_arch = "wasm32")))]
#[path = "realtime/config.rs"]
mod config;
#[cfg(feature = "embedded")]
#[path = "realtime/embedded.rs"]
mod embedded;
#[path = "realtime/gemini.rs"]
mod gemini;
#[path = "realtime/lifecycle.rs"]
mod lifecycle;
#[path = "realtime/openai.rs"]
mod openai;
#[path = "realtime/support.rs"]
mod support;
#[path = "realtime/transport.rs"]
mod transport;
