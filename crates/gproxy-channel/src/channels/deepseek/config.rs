//! Provider configuration and upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "deepseek";
pub const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DeepSeekConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl DeepSeekConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
