//! Provider `config` JSON for Kiro. Unknown keys are ignored.

use crate::channel::{ChannelError, ProviderView};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const ID: &str = "kiro";
/// The Kiro desktop app's own auth host, which mints and refreshes a Builder
/// ID token without an AWS OIDC client (v3 `auth.rs`, `login.rs`).
pub const DEFAULT_AUTH_BASE_URL: &str = "https://prod.us-east-1.auth.desktop.kiro.dev";
/// Both CodeWhisperer planes are regional; `us-east-1` is what the desktop
/// app ships with (v3 `endpoint.rs`).
pub const DEFAULT_REGION: &str = "us-east-1";
/// The IAM Identity Center portal a Builder ID login belongs to (v3
/// `login.rs`); only the authorization-code flow names it.
pub const DEFAULT_START_URL: &str = "https://view.awsapps.com/start";
/// The loopback the Kiro CLI registers for the IdC authorization code.
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth/callback";
/// The three CodeWhisperer scopes the CLI asks for (v3 `login.rs`).
pub const OAUTH_SCOPE: &str =
    "codewhisperer:completions codewhisperer:analysis codewhisperer:conversations";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct KiroConfig {
    /// Region for both `runtime.{region}.kiro.dev` and
    /// `management.{region}.kiro.dev`, and for the AWS OIDC host an IAM
    /// Identity Center credential refreshes against.
    pub region: String,
    /// The Kiro desktop auth host the device login and its refresh talk to.
    pub auth_base_url: String,
    /// Origin serving the management plane (the model catalogue and the usage
    /// limits). Defaults to `management.{region}.kiro.dev`; the provider's own
    /// `base_url` column serves the runtime plane.
    pub management_base_url: Option<String>,
    /// CodeWhisperer profile ARN for accounts whose token does not carry one.
    /// A credential's own `profile_arn` wins.
    pub profile_arn: Option<String>,
    /// Which identity provider the desktop device login opens: `github` or
    /// `google` (v3's `login_provider` login parameter, which v4 has no place
    /// for — a login takes no per-attempt parameters).
    pub login_provider: String,
    /// An IAM Identity Center client the operator registered once with AWS
    /// OIDC `RegisterClient`. Without the pair there is no authorization-code
    /// login; the device login needs neither.
    pub sso_client_id: Option<String>,
    pub sso_client_secret: Option<String>,
    /// The IdC portal the authorization code is issued by.
    pub sso_start_url: Option<String>,
    /// Replaces the SDK user agent on every request.
    pub user_agent: Option<String>,
    /// Static headers added to every request.
    pub headers: BTreeMap<String, String>,
}

impl Default for KiroConfig {
    fn default() -> Self {
        Self {
            region: DEFAULT_REGION.into(),
            auth_base_url: DEFAULT_AUTH_BASE_URL.into(),
            management_base_url: None,
            profile_arn: None,
            login_provider: "github".into(),
            sso_client_id: None,
            sso_client_secret: None,
            sso_start_url: None,
            user_agent: None,
            headers: BTreeMap::new(),
        }
    }
}

impl KiroConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// The region, refused rather than interpolated when it is not one.
    pub(super) fn region(&self) -> Result<&str, ChannelError> {
        let region = self.region.trim();
        let region = if region.is_empty() {
            DEFAULT_REGION
        } else {
            region
        };
        validate_region(region)?;
        Ok(region)
    }

    pub(super) fn auth_base_url(&self) -> &str {
        let base = self.auth_base_url.trim();
        if base.is_empty() {
            DEFAULT_AUTH_BASE_URL
        } else {
            base.trim_end_matches('/')
        }
    }

    /// The identity provider name the device authorization takes, in the
    /// upstream's own capitalisation (v3 `login.rs`).
    pub(super) fn login_provider(&self) -> &'static str {
        match self.login_provider.trim().to_ascii_lowercase().as_str() {
            "google" => "Google",
            _ => "Github",
        }
    }

    /// The IdC portal an authorization-code login is issued by.
    pub(super) fn start_url(&self) -> &str {
        self.sso_start_url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .unwrap_or(DEFAULT_START_URL)
    }
}

/// A region goes into a host name, so it may only be what a region looks like
/// (v3 `endpoint.rs::validate_region`).
pub(super) fn validate_region(region: &str) -> Result<(), ChannelError> {
    if !region.is_empty()
        && region
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        Ok(())
    } else {
        Err(ChannelError::InvalidConfig(format!(
            "`{region}` is not an AWS region"
        )))
    }
}
