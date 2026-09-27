//! OpenAI's own platform at `api.openai.com`, paid for with a platform API
//! key.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/openai` on
//! `main`) and OpenAI's API reference. The key travels in
//! `Authorization: Bearer`, and the routes are OpenAI's own, so the client's
//! native path is forwarded as written: this channel exists for the surface
//! `custom` cannot express rather than for a different base URL.
//!
//! That surface is three things. Responses can be spoken over a WebSocket as
//! well as over HTTP SSE, which needs a `wss://` handshake and the
//! `responses_websockets` and `responses_multi_agent` betas. Realtime is a
//! WebSocket of its own whose query must name either the model or the call
//! being joined, and which never carries the client's credential onward.
//! And a Chat Completions stream only ends with a usage chunk when the
//! request asked for one, which is what makes the call meterable.
//!
//! Account quota has two sources. Every reply carries
//! `x-ratelimit-{limit,remaining,reset}-{requests,tokens}`, whose `reset` is
//! a duration (`2m59.56s`) rather than an instant, and the organization's
//! daily spend is at `/v1/organization/costs`, which needs an Admin key:
//! `secret.quota_api_key` holds it, and without one there is nothing to ask.
//!
//! The credential is `{"api_key": "...", "quota_api_key": "..."}`. There is
//! no login, no refresh and no client fingerprint to impersonate, so
//! `default_connection` stays `None` and the host's own profile decides.

mod config;
mod quota;
mod request;

pub use config::{DEFAULT_BASE_URL, ID, OpenAiConfig, QUOTA_DEFAULT_BASE_URL};
pub use request::{CLIENT_HEADERS, RESPONSES_MULTI_AGENT_BETA, RESPONSES_WS_BETA};

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenAi;
