use gproxy_client::OutboundClient;
use serde_json::Value;

/// Public provider configuration. An upstream kind decodes `config` into its
/// own typed settings; secrets belong exclusively to CredentialView.
#[derive(Clone, Copy)]
pub struct ProviderView<'a> {
    pub id: &'a str,
    pub channel: &'a str,
    pub base_url: Option<&'a str>,
    pub config: &'a Value,
}

/// A decrypted, host-authorized credential borrowed for this binding.
/// Intentionally not Debug or serializable. The host owns decryption and checks
/// owner permissions, enabled state and expiry before binding.
#[derive(Clone, Copy)]
pub struct CredentialView<'a> {
    pub id: &'a str,
    pub provider_id: &'a str,
    pub auth_kind: &'a str,
    pub secret: &'a Value,
    /// Public, non-secret facts the host recorded for this credential, such
    /// as the plan a login discovered.
    pub metadata: &'a Value,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
}

/// The caller's selected account and client, shared by refresh, quotas and CLI
/// services. This is passed through directly, without additional binding checks.
#[derive(Clone, Copy)]
pub struct CredentialContext<'a> {
    pub provider: ProviderView<'a>,
    pub credential: CredentialView<'a>,
    pub client: &'a dyn OutboundClient,
}

/// Transport defaults vary between API calls and browser session exchanges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPurpose {
    Request,
    Browser,
}
