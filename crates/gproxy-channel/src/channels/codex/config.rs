//! Provider configuration and the default Codex transport.

use crate::channel::{ChannelError, ProviderView};
use gproxy_client::{Backend, ConnectionConfig};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "codex";
pub const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_ISSUER: &str = "https://auth.openai.com";
pub const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";
/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CodexConfig {
    /// OAuth issuer for login and refresh.
    pub issuer: String,
    /// The `originator` the backend sees; Codex CLI's by default.
    pub originator: String,
    /// Replaces the CLI-shaped `User-Agent`
    /// (`codex_cli_rs/<version> (<os> <version>; <arch>) <terminal>`).
    pub user_agent: Option<String>,
    /// Static headers added to every backend request.
    pub headers: BTreeMap<String, String>,
    /// Place `prompt_cache_breakpoint` where a client embeds a magic cache
    /// string in Responses bodies (`channels::shared::cache`). Off by
    /// default; the strings are stripped either way.
    pub enable_openai_magic_cache: bool,
    /// Give Responses calls from clients that are not the Codex CLI the
    /// CLI's session, thread, window and turn identity headers, and replay
    /// the backend's `x-codex-turn-state` within a turn (`identity.rs`).
    /// On by default. Headers a client sends itself pass through either way.
    pub synthesize_cli_identity: bool,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            issuer: DEFAULT_ISSUER.into(),
            originator: DEFAULT_ORIGINATOR.into(),
            user_agent: None,
            headers: BTreeMap::new(),
            enable_openai_magic_cache: false,
            synthesize_cli_identity: true,
        }
    }
}

impl CodexConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

/// The Codex CLI's transport: reqwest 0.12 with its default features, so
/// native TLS (OpenSSL on Linux) for HTTP with h2's stock SETTINGS, no
/// response decompression, redirects followed; WebSocket over rustls
/// (`tokio-tungstenite`), which is what the pool's WebSocket client of this
/// backend is. The default client for providers that name no connection
/// profile. Host profiles override it whole.
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::ReqwestNative,
        emulation: None,
        redirect_max_hops: 10,
        ..ConnectionConfig::default()
    }
}
