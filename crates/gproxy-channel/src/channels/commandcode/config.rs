use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const ID: &str = "commandcode";
pub const DEFAULT_BASE_URL: &str = "https://api.commandcode.ai";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CommandCodeConfig {
    pub headers: BTreeMap<String, String>,
    pub project_slug: String,
    pub server_config: serde_json::Map<String, Value>,
    pub zero_data_retention: bool,
}

impl Default for CommandCodeConfig {
    fn default() -> Self {
        Self {
            headers: BTreeMap::new(),
            project_slug: "gproxy".into(),
            server_config: Default::default(),
            zero_data_retention: false,
        }
    }
}

impl CommandCodeConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }
}

pub(super) fn base_url(provider: ProviderView<'_>) -> &str {
    provider
        .base_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

pub(super) fn api_key(secret: &Value) -> Result<&str, ChannelError> {
    // Also accept the CLI's auth.json shape when importing a credential.
    secret
        .get("api_key")
        .or_else(|| secret.get("apiKey"))
        .or_else(|| secret.get("access_token"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}
