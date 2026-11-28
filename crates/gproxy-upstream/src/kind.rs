use gproxy_client::ConnectionConfig;
use serde_json::Value;

use crate::{
    ConfigKey, ConnectionPurpose, CredentialRefresh, OAuthAuthorizationCode, OAuthDeviceCode,
    UpstreamError,
};

/// How a credential for an upstream comes into existence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LoginMode {
    /// Browser redirect with PKCE; `OAuthAuthorizationCode`.
    AuthorizationCode,
    /// User code shown, host polls; `OAuthDeviceCode`.
    DeviceCode,
    /// A secret the operator pastes. No trait is involved: the secret is
    /// stored as given and the upstream kind injects it at request time.
    ApiKey,
}

/// One named way into a login flow, for an upstream whose single flow fronts
/// several identity providers (Kiro's device login opens GitHub or Google; its
/// authorization code is AWS Builder ID or Identity Center). The host lists
/// variants instead of bare modes when a kind declares any, and hands the
/// chosen id to the flow as `LoginContext::variant`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LoginVariant {
    /// Stable id the login request names; unique within the kind.
    pub id: &'static str,
    /// What a UI shows. Usually a product name, so it is not translated.
    pub label: &'static str,
    /// The flow this variant runs.
    pub mode: LoginMode,
}

/// One implementation a provider row can name: how its accounts sign in, how
/// their credentials are renewed and checked, and which outbound client they
/// expect. What the upstream is then asked to do (models, tools) is the
/// business of the trait that builds on this one.
pub trait UpstreamKind: Send + Sync {
    /// Matches the provider's `channel` column.
    fn id(&self) -> &'static str;

    /// The name a management UI shows for this kind.
    fn display_name(&self) -> &'static str {
        self.id()
    }

    /// The sign-in flows offered, in the order a UI lists them. The default
    /// reads the OAuth accessors; a kind that takes a pasted secret adds
    /// `LoginMode::ApiKey`.
    fn login_modes(&self) -> Vec<LoginMode> {
        let mut modes = Vec::new();
        if self.oauth_authorization_code().is_some() {
            modes.push(LoginMode::AuthorizationCode);
        }
        if self.oauth_device_code().is_some() {
            modes.push(LoginMode::DeviceCode);
        }
        modes
    }

    /// Named choices inside the login flows, in the order a UI lists them.
    /// Empty (the default) means each mode is one choice. Every variant's
    /// mode must be one of `login_modes`.
    fn login_variants(&self) -> Vec<LoginVariant> {
        Vec::new()
    }

    /// Stored credential types accepted on manual creation.
    fn credential_kinds(&self) -> Vec<&'static str> {
        let mut kinds = Vec::new();
        for mode in self.login_modes() {
            let kind = match mode {
                LoginMode::ApiKey => "api_key",
                LoginMode::AuthorizationCode | LoginMode::DeviceCode => "oauth",
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
        if kinds.is_empty() {
            kinds.push("api_key");
        }
        kinds
    }

    /// The provider configuration keys this kind reads, for a form to offer.
    fn config_keys(&self) -> Vec<ConfigKey> {
        Vec::new()
    }

    /// Validate manually supplied secret material without making an upstream request.
    /// Kinds with structured cloud credentials override this at the storage boundary.
    fn validate_credential_secret(&self, _secret: &Value) -> Result<(), UpstreamError> {
        Ok(())
    }

    /// Expiry required by newly supplied material, e.g. an unexchanged service-account
    /// key. Returning zero schedules refresh through the normal request-time flow.
    fn initial_credential_expiry(&self, _secret: &Value) -> Option<i64> {
        None
    }

    /// The outbound client this upstream expects when nothing names one: the
    /// host resolves credential profile → provider profile → Setting default →
    /// this → `ConnectionConfig::default()`. Kinds whose upstream fingerprints
    /// its clients return the captured client identity; any explicit host
    /// profile on the credential or provider still wins.
    fn default_connection(&self) -> Option<ConnectionConfig> {
        None
    }

    /// A login may need a browser transport while normal traffic does not.
    fn default_connection_for(&self, purpose: ConnectionPurpose) -> Option<ConnectionConfig> {
        let _ = purpose;
        self.default_connection()
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        None
    }

    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        None
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        None
    }
}
