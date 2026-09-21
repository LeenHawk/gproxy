//! Provider configuration, the two tiers and the upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "opencode";
/// The Zen tier's origin, `/v1` included (v3 `opencode/prepare.rs`).
pub const ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
/// The Go tier's origin; a different path on the same host.
pub const GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
/// Where the device login and the refresh talk (v3 `opencode/login.rs`).
pub const DEFAULT_CONSOLE_BASE_URL: &str = "https://console.opencode.ai";
/// The OAuth client the `opencode` CLI presents.
pub const DEFAULT_CLIENT_ID: &str = "opencode-cli";

/// Which OpenCode subscription a provider row fronts. The two are separate
/// origins with separate quota, not one origin with two paths.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// opencode.ai/zen/v1, billed from a Console wallet.
    #[default]
    Zen,
    /// opencode.ai/zen/go/v1, a subscription with reported usage windows.
    Go,
}

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct OpenCodeConfig {
    pub tier: Tier,
    /// Where the device login and the refresh talk.
    pub console_base_url: String,
    pub client_id: String,
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
    /// Place `cache_control` where a client embeds a magic cache string in a
    /// Claude-dialect body. Off by default; the strings are stripped anyway.
    pub enable_claude_magic_cache: bool,
    /// Place `prompt_cache_breakpoint` where a client embeds a magic cache
    /// string in an OpenAI Chat or Responses body.
    pub enable_openai_magic_cache: bool,
}

impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self {
            tier: Tier::Zen,
            console_base_url: DEFAULT_CONSOLE_BASE_URL.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            headers: BTreeMap::new(),
            enable_claude_magic_cache: false,
            enable_openai_magic_cache: false,
        }
    }
}

impl OpenCodeConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    pub(super) fn client_id(&self) -> &str {
        let id = self.client_id.trim();
        if id.is_empty() { DEFAULT_CLIENT_ID } else { id }
    }

    /// The console origin the login used, when one was recorded, else the
    /// configured one, else OpenCode's.
    pub(super) fn console_base_url(&self, recorded: Option<&str>) -> String {
        recorded
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .unwrap_or_else(|| {
                let configured = self.console_base_url.trim();
                if configured.is_empty() {
                    DEFAULT_CONSOLE_BASE_URL
                } else {
                    configured
                }
            })
            .trim_end_matches('/')
            .to_owned()
    }

    /// Provider column first, then the tier's own origin.
    pub(super) fn base_url(&self, provider: ProviderView<'_>) -> String {
        provider
            .base_url
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .unwrap_or(match self.tier {
                Tier::Zen => ZEN_BASE_URL,
                Tier::Go => GO_BASE_URL,
            })
            .trim_end_matches('/')
            .to_owned()
    }
}
