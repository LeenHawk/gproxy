//! Provider configuration, the two tiers and the upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

/// The pay-as-you-go channel: the Console wallet, the full model list, and
/// the Console account login.
pub const ZEN_ID: &str = "opencodezen";
/// The subscription channel: open models only, reported usage windows, and a
/// pasted key only — the v2 client offers no account login for Go.
pub const GO_ID: &str = "opencodego";
/// The Zen tier's origin, `/v1` included (v3 `opencode/prepare.rs`).
pub const ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
/// The Go tier's origin; a different path on the same host.
pub const GO_BASE_URL: &str = "https://opencode.ai/zen/go/v1";
/// Where the device login and the refresh talk. The Console moved under the
/// main host; `console.opencode.ai` (v3) now redirects its pages here
/// (opencode `packages/core/src/plugin/provider/opencode.ts`).
pub const DEFAULT_CONSOLE_BASE_URL: &str = "https://opencode.ai/console";
/// The OAuth client the `opencode` CLI presents.
pub const DEFAULT_CLIENT_ID: &str = "opencode-cli";

/// Which OpenCode product a channel fronts. The two share a key format and a
/// host, but not an origin, a model list, a bill or a login: the Console
/// routes on the path and checks the Go subscription per workspace member.
/// v3 had them as two channels; v4 briefly merged them behind a `tier` key,
/// which let a Go row offer a login Go has no use for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// opencode.ai/zen/v1, billed from a Console wallet.
    Zen,
    /// opencode.ai/zen/go/v1, a subscription with reported usage windows.
    Go,
}

impl Tier {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Zen => ZEN_ID,
            Self::Go => GO_ID,
        }
    }

    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Zen => ZEN_BASE_URL,
            Self::Go => GO_BASE_URL,
        }
    }
}

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct OpenCodeConfig {
    /// Where the device login and the refresh talk. Zen only.
    pub console_base_url: String,
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
            console_base_url: DEFAULT_CONSOLE_BASE_URL.into(),
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
    pub(super) fn base_url(provider: ProviderView<'_>, tier: Tier) -> String {
        provider
            .base_url
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .unwrap_or(tier.default_base_url())
            .trim_end_matches('/')
            .to_owned()
    }
}
