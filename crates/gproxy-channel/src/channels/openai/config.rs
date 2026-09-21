//! Provider configuration for the OpenAI platform channel.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "openai";
pub const DEFAULT_BASE_URL: &str = "https://api.openai.com";
/// Where the organization cost report lives when nothing else is configured.
pub const QUOTA_DEFAULT_BASE_URL: &str = "https://api.openai.com";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
    /// Place `prompt_cache_breakpoint` where a client embeds a magic cache
    /// string in a Chat Completions or Responses body
    /// (`channels::shared::cache`). Off by default; the strings are stripped
    /// either way.
    pub enable_openai_magic_cache: bool,
    /// Origin of the organization cost report when it differs from the
    /// inference origin.
    pub quota_base_url: Option<String>,
}

impl OpenAiConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
