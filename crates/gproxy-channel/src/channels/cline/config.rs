//! Provider configuration and the upstream constants behind it.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "cline";
/// Cline's own API, which already carries `/api/v1` (v3 `cline/prepare.rs`).
pub const DEFAULT_BASE_URL: &str = "https://api.cline.bot/api/v1";
/// The WorkOS application the Cline CLI authenticates against (v3
/// `cline/login.rs`).
pub const DEFAULT_CLIENT_ID: &str = "client_01K3A541FN8TA3EPPHTD2325AR";
pub const DEFAULT_DEVICE_AUTHORIZATION_URL: &str =
    "https://api.workos.com/user_management/authorize/device";
pub const DEFAULT_TOKEN_URL: &str = "https://api.workos.com/user_management/authenticate";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ClineConfig {
    /// Where the device login starts.
    pub device_authorization_url: String,
    /// Where the device login polls for the WorkOS token pair.
    pub token_url: String,
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl Default for ClineConfig {
    fn default() -> Self {
        Self {
            device_authorization_url: DEFAULT_DEVICE_AUTHORIZATION_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            headers: BTreeMap::new(),
        }
    }
}

impl ClineConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn or_default<'a>(value: &'a str, fallback: &'a str) -> &'a str {
        let value = value.trim();
        if value.is_empty() { fallback } else { value }
    }



    pub(super) fn device_authorization_url(&self) -> &str {
        Self::or_default(
            &self.device_authorization_url,
            DEFAULT_DEVICE_AUTHORIZATION_URL,
        )
    }

    pub(super) fn token_url(&self) -> &str {
        Self::or_default(&self.token_url, DEFAULT_TOKEN_URL)
    }
}

/// The provider's origin, trailing slash removed.
pub(super) fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}
