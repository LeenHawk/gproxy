//! What a channel is, as data: identity, how a person logs in, which optional
//! abilities it implements and which provider configuration keys it reads.
//!
//! This is the catalog a management UI renders and validates a provider form
//! against. It is derived from the channel, never stored, so it cannot drift
//! from the code: the default implementation reads the capability accessors,
//! and a channel only spells out what those cannot tell (its display name, the
//! keys it decodes out of the provider `config` JSON).
//!
//! # Why these types carry `serde` and `ts-rs` attributes
//!
//! They are the only part of this crate a management UI reads, and it reads
//! them beside the host's own configuration DTOs, so they follow that
//! convention rather than Rust's: struct fields serialize `camelCase`, enum
//! variants `snake_case`. The `ts` feature derives the TypeScript declarations
//! from the same definitions — a console that renders a provider form off a
//! descriptor cannot then be typed against a hand-written copy of it.

use gproxy_upstream::{ConfigKey, ConfigKeyKind, LoginMode, LoginVariant, UpstreamKind};

/// Optional abilities beyond protocol operations. All false is a legitimate
/// channel: an API-key upstream needs none of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChannelCapabilities {
    /// `CredentialRefresh`: the credential can be renewed without a person.
    pub refresh: bool,
    /// `QuotaQuery`: the upstream can be asked about the account's balance.
    pub quota_query: bool,
    /// `QuotaReset`: a spent window can be reopened against the upstream.
    pub quota_reset: bool,
    /// `ChannelServices`: non-protocol endpoints of the upstream's own client.
    pub services: bool,
    /// The channel serves at least one operation over a WebSocket. Not
    /// derivable from an accessor; a channel that overrides a socket operation
    /// declares it.
    pub websocket: bool,
    /// `QuotaModel::allows_paid_usage` reads the credential metadata flag
    /// `allow_paid_usage`, so the console offers the switch. Not derivable
    /// either: the method has a default, and overriding it is invisible here.
    pub paid_usage: bool,
}

/// Read from the `config` JSON for every channel, by the host rather than by
/// the channel itself: `credential_strategy` selects among the provider's
/// credentials, `allowed_headers` adds to the global and channel header sets.
pub const HOST_CONFIG_KEYS: [ConfigKey; 3] = [
    ConfigKey::optional(
        "forward_thinking",
        ConfigKeyKind::Bool,
        "Forward thinking blocks from request history. Defaults to true; disabling removes reasoning content and signatures without disabling model thinking.",
    ),
    ConfigKey::optional(
        "credential_strategy",
        ConfigKeyKind::String,
        "How the host picks among this provider's credentials.",
    ),
    ConfigKey::optional(
        "allowed_headers",
        ConfigKeyKind::HeaderList,
        "Additional client headers to forward, unioned with the global allow-list and channel defaults.",
    ),
];

/// A channel as the management layer sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ChannelDescriptor {
    /// Matches `BaseChannel::id` and the provider's `channel` column.
    pub id: &'static str,
    pub display_name: &'static str,
    /// Empty means no login flow is offered for this channel.
    pub login_modes: Vec<LoginMode>,
    /// Named choices inside those flows; empty means each mode is one choice
    /// (`UpstreamKind::login_variants`).
    pub login_variants: Vec<LoginVariant>,
    /// Stored credential types accepted on manual creation, distinct from login flows.
    pub credential_kinds: Vec<&'static str>,
    pub capabilities: ChannelCapabilities,
    pub config_keys: Vec<ConfigKey>,
}

impl ChannelDescriptor {
    /// An upstream kind that is not a channel, in the same catalog: it has
    /// accounts and a sign-in, and no model side to describe.
    pub fn of_upstream(kind: &dyn UpstreamKind) -> Self {
        Self {
            id: kind.id(),
            display_name: kind.display_name(),
            login_variants: kind.login_variants(),
            login_modes: kind.login_modes(),
            credential_kinds: kind.credential_kinds(),
            capabilities: ChannelCapabilities {
                refresh: kind.credential_refresh().is_some(),
                ..Default::default()
            },
            config_keys: kind.config_keys(),
        }
    }

    /// Convenience for tests and UIs that look one key up by name.
    pub fn config_key(&self, name: &str) -> Option<&ConfigKey> {
        self.config_keys.iter().find(|key| key.name == name)
    }
}
