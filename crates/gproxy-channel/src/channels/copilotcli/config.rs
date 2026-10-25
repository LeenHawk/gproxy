//! Provider configuration, the editor identity it can restate, and the
//! upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "copilotcli";

/// The inference origin for each kind of Copilot seat (v3
/// `copilotcli/auth.rs::account_base`). An operator override only: the CLI
/// itself reads the origin from `copilot_internal/user` (`endpoints.api`).
pub const INDIVIDUAL_BASE_URL: &str = "https://api.githubcopilot.com";
pub const BUSINESS_BASE_URL: &str = "https://api.business.githubcopilot.com";
pub const ENTERPRISE_BASE_URL: &str = "https://api.enterprise.githubcopilot.com";
/// Where the account probe `copilot_internal/user` lives.
pub const DEFAULT_GITHUB_API_URL: &str = "https://api.github.com";
/// The GitHub device flow the CLI uses (Copilot CLI 1.0.77 `runtime.node`).
pub const DEFAULT_DEVICE_AUTHORIZATION_URL: &str = "https://github.com/login/device/code";
pub const DEFAULT_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// The OAuth app the Copilot CLI ships; an identifier, not a secret. The
/// strings run `client_id Ov23ctDVkRmgkPke0Mmm
/// read:user,read:org,repo,gist,codespace /login/device/code` in 1.0.77's
/// `runtime.node`.
pub const DEFAULT_CLIENT_ID: &str = "Ov23ctDVkRmgkPke0Mmm";
/// The CLI's device-flow scope, comma-separated as it sends it.
pub const DEFAULT_SCOPE: &str = "read:user,read:org,repo,gist,codespace";

/// The banner the CLI sends on every call, to GitHub and to Copilot alike.
pub const CLI_USER_AGENT: &str = "copilot/1.0.77 (linux v24.16.0) term/unknown";
/// The media type the CLI's GitHub REST client asks for.
pub const GITHUB_ACCEPT: &str = "application/vnd.github.v3+json";
/// Which Copilot client the request is billed as.
pub const INTEGRATION_ID: &str = "copilot-developer-cli";
/// What the CLI says the completion is for.
pub const OPENAI_INTENT: &str = "conversation-agent";

/// Which Copilot seat this provider fronts; the seat decides the origin.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountType {
    #[default]
    Individual,
    Business,
    Enterprise,
}

impl AccountType {
    pub(super) fn base_url(self) -> &'static str {
        match self {
            Self::Individual => INDIVIDUAL_BASE_URL,
            Self::Business => BUSINESS_BASE_URL,
            Self::Enterprise => ENTERPRISE_BASE_URL,
        }
    }
}

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CopilotCliConfig {
    /// An operator override for the seat's origin. Without one the origin
    /// the account probe reported (`endpoints.api`) is used.
    pub account_type: Option<AccountType>,
    /// Where the account is probed.
    pub github_api_url: String,
    pub device_authorization_url: String,
    pub token_url: String,
    /// The scope the device login asks for.
    pub scope: String,
    /// Static headers added to every inference request.
    pub headers: BTreeMap<String, String>,
}

impl Default for CopilotCliConfig {
    fn default() -> Self {
        Self {
            account_type: None,
            github_api_url: DEFAULT_GITHUB_API_URL.into(),
            device_authorization_url: DEFAULT_DEVICE_AUTHORIZATION_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            scope: DEFAULT_SCOPE.into(),
            headers: BTreeMap::new(),
        }
    }
}

impl CopilotCliConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn or_default<'a>(value: &'a str, fallback: &'a str) -> &'a str {
        let value = value.trim();
        if value.is_empty() { fallback } else { value }
    }

    pub(super) fn github_api_url(&self) -> &str {
        Self::or_default(&self.github_api_url, DEFAULT_GITHUB_API_URL).trim_end_matches('/')
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

    pub(super) fn scope(&self) -> &str {
        Self::or_default(&self.scope, DEFAULT_SCOPE)
    }
}
