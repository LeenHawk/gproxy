//! Provider configuration for the Anthropic API channel.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "claudeapi";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// Where the organization cost report lives when nothing else is configured.
pub const QUOTA_DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

pub use crate::channels::shared::claude_fallback::FallbackMode;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ClaudeapiConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
    /// Place `cache_control` where a client embeds a magic cache string in a
    /// Messages or count_tokens body (`channels::shared::cache`). Off by
    /// default; the strings are stripped either way.
    pub enable_claude_magic_cache: bool,
    /// Server-side fallback for Messages requests that name none themselves.
    pub fallback_mode: FallbackMode,
    /// The chain `fallback_mode: "models"` installs, longest first. At most
    /// three survive, which is what the API accepts.
    pub fallback_models: Vec<String>,
    /// Origin of the organization cost report when it is not the inference
    /// origin, e.g. a relay that only proxies the Admin API.
    pub quota_base_url: Option<String>,
}

impl ClaudeapiConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
