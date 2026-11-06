//! OpenAI Codex: a ChatGPT account used through the Codex backend.
//!
//! Wire facts follow the Codex CLI (`samples/codex`, source audit at
//! `upstream_docs/codex/protocol.md`, main `67f0994b53`): OAuth at
//! `auth.openai.com` (authorization code with PKCE, and the device-code flow
//! under `/api/accounts/deviceauth`), refresh at `/oauth/token`, generation
//! at `{base}/responses` over HTTP SSE or WebSocket, account limits in the
//! `x-<limit>-primary/secondary-*` header families and at
//! `/backend-api/wham/usage`. The CLI's other backend calls (plugins, MCP,
//! settings, files, remote control, `whoami`) are `ChannelServices` in
//! `services.rs`. The credential secret is an `OAuthCredential`;
//! the account id and plan discovered at login travel in `provider_fields`
//! and, once the host persists them, in the credential's metadata.
//! Refresh scheduling, partial token updates and failure recovery follow
//! `codex-rs/login/src/auth/manager.rs`; request reasons follow
//! `codex-rs/login/src/oauth/client.rs` in the same sample tree.
//!
//! Device/account projections are owned by the selected credential: the
//! channel replaces installation ids in headers and Responses metadata,
//! including WebSocket request frames, while retaining client thread, turn,
//! window and subagent metadata. This account-scoped installation policy is
//! intentional; the official CLI persists one random id per `CODEX_HOME`.

mod agent;
mod common;
mod config;
mod headers;
mod identity;
mod models;
mod oauth;
mod quota;
mod realtime;
mod request;
mod services;
mod shape;
mod sse;
mod wire;

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
