//! Provider configuration. Every default is the value the reference client
//! puts on the wire; an operator only touches these when the upstream moves.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::channel::{ChannelError, ProviderView};

pub const ID: &str = "devin";

/// `ClientMetadata` #1 and #12 on every captured request: the CLI's internal
/// client name (`devin-connect.js`, the `CLIENT_NAME` constant). The account
/// endpoints in `samples/cpa-manager-plus` send the same name.
pub const CLIENT_NAME: &str = "chisel";
/// `ClientMetadata` #2 and #7: the CLI build string the capture carried
/// (`devin-connect.js`, `CLIENT_VERSION`).
pub const CLIENT_VERSION: &str = "2026.8.18";
/// `ClientMetadata` #4 and #5 on the captured request.
pub const CLIENT_LOCALE: &str = "en";
pub const CLIENT_OS: &str = "windows";
/// `CompletionConfig` #2, the enforced output cap when the caller names none.
/// The reference defaults to 8192 rather than a smaller value because #2 only
/// became the real cap once the max_tokens/max_newlines tags were corrected.
pub const DEFAULT_MAX_TOKENS: u64 = 8192;
/// `CompletionConfig` #3 (`max_newlines`), where the reference writes the
/// context window. Keeping its value means the field stays the no-op it has
/// always been on this wire.
pub const DEFAULT_MAX_NEWLINES: u64 = 128_000;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct DevinConfig {
    /// `ClientMetadata` #1 and #12.
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
    /// `CompletionConfig` #3.
    pub max_newlines: u64,
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
            headers: BTreeMap::new(),
        }
    }
}

impl DevinConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
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
    pub fn static_headers(&self, headers: &mut http::HeaderMap) -> Result<(), ChannelError> {
        for (name, value) in &self.headers {
            headers.insert(
                http::HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
                http::HeaderValue::from_str(value)
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
            );
        }
        Ok(())
    }
}
