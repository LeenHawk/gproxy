//! Provider configuration and the upstream constants.

use gproxy_upstream::{ProviderView, UpstreamError};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "factory";
/// The hosted API origin (`FACTORY_API_BASE_URL`'s default in droid 1.18.0).
/// EU accounts are served from `https://api.eu.factory.ai`.
pub const DEFAULT_BASE_URL: &str = "https://api.factory.ai";
/// WorkOS User Management, where the CLI's device login and refresh go.
pub const DEFAULT_WORKOS_BASE_URL: &str = "https://api.workos.com/user_management";
/// The production WorkOS client of the Droid CLI (`x/js/0070.js`, `Lt`).
pub const DEFAULT_CLIENT_ID: &str = "client_01HNM792M5G5G1A2THWPXKFMXB";
/// The CLI release whose wire this channel reproduces.
pub const DEFAULT_CLIENT_VERSION: &str = "1.18.0";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct FactoryConfig {
    /// WorkOS User Management origin for the device login and refresh.
    pub workos_base_url: String,
    /// WorkOS client id the login and refresh present.
    pub client_id: String,
    /// Version reported in `User-Agent` and `x-client-version`.
    pub client_version: String,
    /// Model id to `x-api-provider` value, over the built-in table.
    pub api_providers: BTreeMap<String, String>,
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl Default for FactoryConfig {
    fn default() -> Self {
        Self {
            workos_base_url: DEFAULT_WORKOS_BASE_URL.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            client_version: DEFAULT_CLIENT_VERSION.into(),
            api_providers: BTreeMap::new(),
            headers: BTreeMap::new(),
        }
    }
}

fn or_default<'a>(value: &'a str, default: &'a str) -> &'a str {
    let value = value.trim();
    if value.is_empty() { default } else { value }
}

impl FactoryConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, UpstreamError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| UpstreamError::InvalidConfig(error.to_string()))
    }

    pub(super) fn base_url(provider: ProviderView<'_>) -> String {
        or_default(provider.base_url.unwrap_or_default(), DEFAULT_BASE_URL)
            .trim_end_matches('/')
            .to_owned()
    }

    pub(super) fn workos_base_url(&self) -> String {
        or_default(&self.workos_base_url, DEFAULT_WORKOS_BASE_URL)
            .trim_end_matches('/')
            .to_owned()
    }

    pub(super) fn client_id(&self) -> &str {
        or_default(&self.client_id, DEFAULT_CLIENT_ID)
    }

    pub(super) fn client_version(&self) -> &str {
        or_default(&self.client_version, DEFAULT_CLIENT_VERSION)
    }
}
