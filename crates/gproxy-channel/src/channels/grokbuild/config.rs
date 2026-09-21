//! Provider `config` JSON for Grok Build. Unknown keys are ignored.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "grokbuild";
/// The chat proxy the Grok Build CLI talks to. It is not `api.x.ai`: an
/// OAuth account reaches generation only here (v3 `prepare.rs`).
pub const DEFAULT_BASE_URL: &str = "https://cli-chat-proxy.grok.com/v1";
/// xAI's public origin, which the same account reaches for media.
pub const DEFAULT_MEDIA_BASE_URL: &str = "https://api.x.ai/v1";
/// The CLI's own OAuth client (v3 `login.rs`, `auth.rs`).
pub const DEFAULT_CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
pub const DEFAULT_DEVICE_CODE_URL: &str = "https://auth.x.ai/oauth2/device/code";
pub const DEFAULT_TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
/// Everything the CLI asks for, in one space-separated scope (v3 `login.rs`).
pub const OAUTH_SCOPE: &str = concat!(
    "openid profile email offline_access grok-cli:access api:access ",
    "conversations:read conversations:write workspaces:read workspaces:write"
);

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct GrokBuildConfig {
    /// Origin for xAI's own media paths; the provider's `base_url` column
    /// serves the chat proxy instead.
    pub media_base_url: String,
    /// Origin the billing probe reads; defaults to the chat proxy, which is
    /// the only surface that answers it.
    pub usage_base_url: Option<String>,
    pub oauth_client_id: String,
    pub oauth_device_code_url: String,
    pub oauth_token_url: String,
    /// Replaces the CLI user agent on every request.
    pub user_agent: Option<String>,
    /// Static headers added to every request.
    pub headers: BTreeMap<String, String>,
}

impl Default for GrokBuildConfig {
    fn default() -> Self {
        Self {
            media_base_url: DEFAULT_MEDIA_BASE_URL.into(),
            usage_base_url: None,
            oauth_client_id: DEFAULT_CLIENT_ID.into(),
            oauth_device_code_url: DEFAULT_DEVICE_CODE_URL.into(),
            oauth_token_url: DEFAULT_TOKEN_URL.into(),
            user_agent: None,
            headers: BTreeMap::new(),
        }
    }
}

impl GrokBuildConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    pub(super) fn client_id(&self) -> &str {
        non_empty(&self.oauth_client_id).unwrap_or(DEFAULT_CLIENT_ID)
    }

    pub(super) fn device_code_url(&self) -> &str {
        non_empty(&self.oauth_device_code_url).unwrap_or(DEFAULT_DEVICE_CODE_URL)
    }

    pub(super) fn token_url(&self) -> &str {
        non_empty(&self.oauth_token_url).unwrap_or(DEFAULT_TOKEN_URL)
    }

    pub(super) fn media_base_url(&self) -> &str {
        non_empty(&self.media_base_url)
            .unwrap_or(DEFAULT_MEDIA_BASE_URL)
            .trim_end_matches('/')
    }

    /// The billing surface: configured, else the chat proxy this provider
    /// already uses.
    pub(super) fn usage_base_url(&self, provider: ProviderView<'_>) -> String {
        self.usage_base_url
            .as_deref()
            .and_then(non_empty)
            .map(|base| base.trim_end_matches('/').to_owned())
            .unwrap_or_else(|| base_url(provider))
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

/// The chat proxy origin: the provider's own column, then the CLI's.
pub(super) fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}
