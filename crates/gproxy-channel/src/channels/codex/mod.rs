//! OpenAI Codex: a ChatGPT account used through the Codex backend.
//!
//! Wire facts follow the Codex CLI (`samples/codex`): OAuth at
//! `auth.openai.com` (authorization code with PKCE, and the device-code flow
//! under `/api/accounts/deviceauth`), refresh at `/oauth/token`, generation
//! at `{base}/responses` over HTTP SSE or WebSocket, account limits in the
//! `x-<limit>-primary/secondary-*` header families and at
//! `/backend-api/wham/usage`. The CLI's other backend calls (plugins, MCP,
//! settings, files, remote control, `whoami`) are `ChannelServices` in
//! `services.rs`. The credential secret is an `OAuthCredential`;
//! the account id and plan discovered at login travel in `provider_fields`
//! and, once the host persists them, in the credential's metadata.

mod agent;
mod common;
mod config;
mod headers;
mod identity;
mod oauth;
mod quota;
mod request;
mod services;
mod shape;
mod sse;
mod usage;

pub use agent::CLI_VERSION;
pub use config::{
    CodexConfig, DEFAULT_BASE_URL, DEFAULT_CLIENT_ID, DEFAULT_ISSUER, DEFAULT_ORIGINATOR, ID,
    default_connection,
};
pub use headers::CLI_HEADERS;
pub use services::{
    KIND_ENVIRONMENT, KIND_FILE, KIND_PLUGIN, KIND_REMOTE_SERVER, KIND_TASK, service_routes,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct Codex;
