//! Anthropic's first-party API at `api.anthropic.com`, paid for with an
//! Anthropic API key.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/claudeapi`
//! and `shared/claude/*` on `main`) and Anthropic's API reference. The key
//! travels in `x-api-key` with `anthropic-version: 2023-06-01`; the routes are
//! `/v1/models`, `/v1/models/{model}`, `/v1/messages`,
//! `/v1/messages/count_tokens` and, on the OpenAI SDK compatibility layer,
//! `/v1/chat/completions`. The channel picks the route from the operation
//! rather than from the client's path, which is what `custom` cannot do.
//!
//! Three body behaviours come with the Messages route. Sampling parameters
//! are refused by models that do not accept them, and by every model while
//! extended thinking is on, so they are removed unless the model is known to
//! tolerate them. A trailing assistant text turn is a prefill, which the same
//! newer models reject, so it becomes a user turn for them. Two features are
//! requested through `anthropic-beta` rather than through the body, and the
//! body is what says they are wanted: `speed: "fast"` and
//! `thinking.display: "updates"`. Server-side fallback is configuration, not
//! a client concern, so `config.fallback_mode` decides whether a `fallbacks`
//! field and its beta are added to a request that has none.
//!
//! Account quota has two sources. Every reply carries
//! `anthropic-ratelimit-{requests,tokens,input-tokens,output-tokens}-*`, and
//! the organization's daily spend is at `/v1/organizations/cost_report`,
//! which needs an Admin key; `secret.quota_api_key` holds it when the
//! inference key is not one.
//!
//! The credential is `{"api_key": "...", "quota_api_key": "..."}`. There is
//! no login, no refresh and no client fingerprint to impersonate, so
//! `default_connection` stays `None` and the host's own profile decides.

mod config;
use crate::channels::shared::claude_hygiene as hygiene;
mod quota;
mod request;
mod usage;

pub use config::{
    ANTHROPIC_VERSION, ClaudeapiConfig, DEFAULT_BASE_URL, FallbackMode, ID, QUOTA_DEFAULT_BASE_URL,
};
pub use request::CLIENT_HEADERS;

#[derive(Debug, Default, Clone, Copy)]
pub struct Claudeapi;
