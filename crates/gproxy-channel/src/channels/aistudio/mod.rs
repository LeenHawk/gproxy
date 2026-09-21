//! Google AI Studio: the Gemini API at `generativelanguage.googleapis.com`,
//! paid for with a plain API key.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/aistudio` on
//! `main`) and Google's Gemini API reference. One origin serves two surfaces
//! and each wants its own credential header, which is why this is a channel
//! rather than a `custom` provider: the native Gemini methods live under
//! `/v1beta/models/{model}:{method}` and take `x-goog-api-key`, while the
//! OpenAI compatibility layer lives under `/v1beta/openai/**` and takes
//! `Authorization: Bearer`. Client paths addressed at OpenAI's own layout
//! (`/v1/chat/completions`, `/v1/videos/...`) are moved onto that prefix.
//!
//! Streaming has two framings: `streamGenerateContent` answers with a single
//! incrementally delivered JSON array unless the request carries `alt=sse`,
//! and the compatibility layer always answers with SSE. `usage.rs` watches
//! both.
//!
//! The credential is `{"api_key": "..."}`. There is no login, no refresh and
//! no fingerprint to impersonate: this is an ordinary HTTPS API, so
//! `default_connection` stays `None` and the host's own profile decides.

mod config;
mod request;
mod usage;

pub use config::{AistudioConfig, DEFAULT_BASE_URL, ID, OPENAI_PREFIX};
pub use request::CLIENT_HEADERS;
#[derive(Debug, Default, Clone, Copy)]
pub struct Aistudio;
