//! Provider configuration, the editor identity it can restate, and the
//! upstream constants.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "copilotcli";

/// The inference origin for each kind of Copilot seat (v3
/// `copilotcli/auth.rs::account_base`).
pub const INDIVIDUAL_BASE_URL: &str = "https://api.githubcopilot.com";
pub const BUSINESS_BASE_URL: &str = "https://api.business.githubcopilot.com";
pub const ENTERPRISE_BASE_URL: &str = "https://api.enterprise.githubcopilot.com";
/// Where the GitHub token is spent: the Copilot token mint and the account
/// probe both live here (v3 `copilotcli/auth.rs`, `copilotcli/quota.rs`).
pub const DEFAULT_GITHUB_API_URL: &str = "https://api.github.com";
/// The GitHub device flow the CLI uses (v3 `copilotcli/login.rs`).
pub const DEFAULT_DEVICE_AUTHORIZATION_URL: &str = "https://github.com/login/device/code";
pub const DEFAULT_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// The Copilot OAuth app the CLI ships; an identifier, not a secret.
pub const DEFAULT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
/// The only scope the CLI asks for.
pub const DEFAULT_SCOPE: &str = "read:user";

/// The banner the CLI sends on every call, to GitHub and to Copilot alike.
pub const CLI_USER_AGENT: &str = "copilot/1.0.61 (linux v24.16.0) term/unknown";
/// `editor-version` on an inference call: the CLI names itself.
pub const CLI_EDITOR_VERSION: &str = "copilot/1.0.61";
/// `editor-version` on a GitHub call: the CLI claims the editor whose
/// entitlement mints Copilot tokens.
pub const DEFAULT_VSCODE_VERSION: &str = "1.95.3";
pub const EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.43.0";
/// `x-github-api-version` on api.github.com.
pub const GITHUB_API_VERSION: &str = "2025-04-01";
/// `x-github-api-version` on the Copilot inference origin, which pins a
/// different date from the REST API.
pub const COPILOT_API_VERSION: &str = "2026-06-01";
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

    pub(super) fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "business" => Some(Self::Business),
            "enterprise" => Some(Self::Enterprise),
            "individual" => Some(Self::Individual),
            _ => None,
        }
    }
}

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CopilotCliConfig {
    /// The seat this provider fronts. v3 read it off the secret and no login
    /// ever wrote it, so in v4 it is the operator's to state; a host that
    /// later records one on the credential is read first.
    pub account_type: Option<AccountType>,
    /// Where the Copilot token is minted and the account probed.
    pub github_api_url: String,
    /// OAuth client the device login presents; defaults to the CLI's own.
    pub client_id: String,
    pub device_authorization_url: String,
    pub token_url: String,
    /// The scope the device login asks for.
    pub scope: String,
    /// The editor release named in `editor-version` on GitHub calls.
    pub vscode_version: String,
    /// Static headers added to every inference request.
    pub headers: BTreeMap<String, String>,
}

impl Default for CopilotCliConfig {
    fn default() -> Self {
        Self {
            account_type: None,
            github_api_url: DEFAULT_GITHUB_API_URL.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            device_authorization_url: DEFAULT_DEVICE_AUTHORIZATION_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            scope: DEFAULT_SCOPE.into(),
            vscode_version: DEFAULT_VSCODE_VERSION.into(),
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

    pub(super) fn client_id(&self) -> &str {
        Self::or_default(&self.client_id, DEFAULT_CLIENT_ID)
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

    pub(super) fn vscode_version(&self) -> &str {
        Self::or_default(&self.vscode_version, DEFAULT_VSCODE_VERSION)
    }
}
