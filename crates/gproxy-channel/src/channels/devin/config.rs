//! Provider configuration. Every default is the value the reference client
//! puts on the wire; an operator only touches these when the upstream moves.

use std::collections::BTreeMap;

use serde::Deserialize;

use gproxy_upstream::{ProviderView, UpstreamError};

pub const ID: &str = "devin";

/// `ClientMetadata` #12, and #1 on the account calls: the CLI's internal name.
/// Devin CLI 3000.15.14 (captured 2026-11-28) sends it on every call; the
/// chat call also repeats it in #28.
pub const CLIENT_NAME: &str = "chisel";
/// `ClientMetadata` #1 on `GetChatMessage`, where the same CLI names itself
/// by its product name instead (captured 2026-11-28).
pub const CHAT_CLIENT_NAME: &str = "devin-cli";
/// `ClientMetadata` #2 and #7: the CLI build (`devin version`, 3000.15.14).
pub const CLIENT_VERSION: &str = "3000.15.14";
/// `ClientMetadata` #4 and #5 on the captured request.
pub const CLIENT_LOCALE: &str = "en";
/// The CLI's browser PKCE authorization page.
pub const DEFAULT_AUTHORIZE_URL: &str = "https://app.devin.ai/auth/cli/continue";
/// The CLI's authorization-code exchange endpoint.
pub const DEFAULT_TOKEN_URL: &str = "https://api.devin.ai/auth/cli/token";
pub const CLIENT_OS: &str = "windows";
/// `CompletionConfig` #2, the output cap when the caller names none. Devin
/// CLI 3000.15.14 sends 128000 here (captured 2026-11-28).
pub const DEFAULT_MAX_TOKENS: u64 = 128_000;
/// `CompletionConfig` #3, which the reference's `.proto` declares
/// `max_newlines`. The reference wrote its context window (128000) there; Devin
/// CLI 3000.15.14 sends 400 (captured 2026-11-28), so the field is the
/// newline cap its declaration names, not a context size.
pub const DEFAULT_MAX_NEWLINES: u64 = 400;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct DevinConfig {
    /// `ClientMetadata` #12 and #28, and #1 on the account calls.
    pub client_name: String,
    /// `ClientMetadata` #2 and #7.
    pub client_version: String,
    /// `ClientMetadata` #4.
    pub locale: String,
    /// `ClientMetadata` #5.
    pub os: String,
    /// Extra client-facing model name to upstream selector entries, merged
    /// over the built-in catalogue. The escape valve for selectors the
    /// upstream added after the catalogue this channel ships was captured.
    pub models: BTreeMap<String, String>,
    /// `CompletionConfig` #2 when the request names no cap.
    pub max_tokens: u64,
    /// `CompletionConfig` #3; see [`DEFAULT_MAX_NEWLINES`]. The earlier
    /// `context_window` spelling still loads.
    #[serde(alias = "context_window")]
    pub max_newlines: u64,
    /// Browser PKCE authorization endpoint.
    pub authorize_url: String,
    /// Authorization-code exchange endpoint.
    pub token_url: String,
    /// Static headers added to every call.
    pub headers: BTreeMap<String, String>,
}

impl Default for DevinConfig {
    fn default() -> Self {
        Self {
            client_name: CLIENT_NAME.into(),
            client_version: CLIENT_VERSION.into(),
            locale: CLIENT_LOCALE.into(),
            os: CLIENT_OS.into(),
            models: BTreeMap::new(),
            max_tokens: DEFAULT_MAX_TOKENS,
            max_newlines: DEFAULT_MAX_NEWLINES,
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            headers: BTreeMap::new(),
        }
    }
}

impl DevinConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, UpstreamError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| UpstreamError::InvalidConfig(error.to_string()))
    }

    /// The origin, without a trailing slash.
    pub fn base_url(provider: ProviderView<'_>) -> String {
        provider
            .base_url
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .unwrap_or(super::connect::DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_owned()
    }

    /// Apply `config.headers`. These are the provider's own additions and are
    /// written after the channel's transport headers, so an operator can add
    /// what the upstream needs without being able to drop what it requires.
    pub fn static_headers(&self, headers: &mut http::HeaderMap) -> Result<(), UpstreamError> {
        for (name, value) in &self.headers {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| UpstreamError::InvalidConfig(format!("header `{name}`")))?,
                http::HeaderValue::from_str(value)
                    .map_err(|_| UpstreamError::InvalidConfig(format!("header `{name}`")))?,
            );
        }
        Ok(())
    }
}
