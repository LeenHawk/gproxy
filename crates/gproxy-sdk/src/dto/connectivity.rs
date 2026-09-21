//! The two probes and what model discovery finds.
//!
//! Both probes leave this deployment and reach a real network, which is what
//! makes them useful and what makes them different from every other read in
//! this crate. A probe that fails to reach anything is still a successful
//! probe: it answers `ok: false` with the reason, because "the upstream is
//! unreachable" is the result an operator asked for, not an error in asking.

use serde::{Deserialize, Serialize};

use super::UsageTokensDto;

/// What a connectivity test should exercise. Each variant resolves the same
/// client chain a real call would: the credential's profile, then the
/// provider's, then the channel's default, then the instance default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum ConnectivityScope {
    /// The instance default profile, with no provider in the picture.
    Global,
    Provider {
        provider_id: String,
    },
    /// The chain that credential's own calls take.
    Credential {
        credential_id: String,
    },
    /// A proxy that is not configured anywhere yet: the reason this exists is
    /// to test one before saving it.
    Proxy {
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityTest {
    #[serde(flatten)]
    pub scope: ConnectivityScope,
}

/// What the edge looks like from here. `ip` and `colo` come from Cloudflare's
/// trace endpoint, so `ip` is the address the upstream would see — which is
/// the whole point of testing a proxy.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityResultDto {
    pub ok: bool,
    /// Measured across the whole exchange, including the body read, and
    /// reported for a failure too.
    pub latency_ms: u64,
    pub ip: Option<String>,
    /// The Cloudflare edge that answered, e.g. `NRT`.
    pub colo: Option<String>,
    /// Set exactly when `ok` is false.
    pub error: Option<String>,
}

/// A real generation against one provider. It spends a real credential and
/// writes a real usage row; there is no dry run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTest {
    pub provider_id: String,
    /// The upstream model name, as the provider's catalog spells it.
    pub model: String,
    /// The credential to spend. None lets the provider's usual selection
    /// choose, which is what a test of "does this provider work" wants.
    #[serde(default)]
    pub credential_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTestResultDto {
    pub ok: bool,
    pub latency_ms: u64,
    /// The upstream's HTTP status, or 0 when nothing was answered.
    pub status: u16,
    pub model: String,
    /// What the upstream reported it spent. Absent when it reported nothing,
    /// which is not the same as zero.
    pub usage: Option<UsageTokensDto>,
    pub error: Option<String>,
}

/// One model an upstream says it offers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModelDto {
    pub upstream_name: String,
    /// Whether this provider already has a `provider_models` row for it.
    pub known: bool,
    /// Whether the bundled catalog can price it without an operator writing a
    /// rule by hand.
    pub has_default_price: bool,
}
