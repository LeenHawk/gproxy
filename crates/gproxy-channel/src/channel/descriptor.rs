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

/// How a credential for this channel comes into existence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
pub enum LoginMode {
    /// Browser redirect with PKCE; `OAuthAuthorizationCode`.
    AuthorizationCode,
    /// User code shown, host polls; `OAuthDeviceCode`.
    DeviceCode,
    /// A browser session cookie exchanged for a credential; `CookieLogin`.
    Cookie,
    /// A key the operator pastes. No channel trait is involved: the secret is
    /// stored as given and the channel injects it at request time.
    ApiKey,
}

/// Optional abilities beyond protocol operations. All false is a legitimate
/// channel: an API-key upstream needs none of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
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
}

/// How a configuration value is written, for form rendering and validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
pub enum ConfigKeyKind {
    String,
    Bool,
    Integer,
    /// Any other JSON shape: an array, or an object that is not headers.
    Json,
    /// HTTP header names, or header name/value pairs.
    HeaderList,
}

/// One entry of a provider's configuration. Unless noted, the name is a key of
/// the provider's `config` JSON; `base_url` is the provider's own column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConfigKey {
    pub name: &'static str,
    pub kind: ConfigKeyKind,
    /// The provider cannot serve a request without it. Everything else has a
    /// channel default.
    pub required: bool,
    pub description: &'static str,
}

impl ConfigKey {
    pub const fn required(
        name: &'static str,
        kind: ConfigKeyKind,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            required: true,
            description,
        }
    }
    pub const fn optional(
        name: &'static str,
        kind: ConfigKeyKind,
        description: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            required: false,
            description,
        }
    }
}

/// Read from the `config` JSON for every channel, by the host rather than by
/// the channel itself: `credential_strategy` selects among the provider's
/// credentials, `allowed_headers` narrows what `HeaderAllowlist` forwards.
pub const HOST_CONFIG_KEYS: [ConfigKey; 3] = [
    ConfigKey::optional(
        "credential_strategy",
        ConfigKeyKind::String,
        "How the host picks among this provider's credentials.",
    ),
    ConfigKey::optional(
        "session_affinity",
        ConfigKeyKind::Bool,
        "Reuse a session credential while usable; defaults on for legacy sticky strategies.",
    ),
    ConfigKey::optional(
        "allowed_headers",
        ConfigKeyKind::HeaderList,
        "The only client headers forwarded upstream; absent forwards the channel's own set.",
    ),
];

/// A channel as the management layer sees it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ChannelDescriptor {
    /// Matches `BaseChannel::id` and the provider's `channel` column.
    pub id: &'static str,
    pub display_name: &'static str,
    /// Empty means no login flow is offered for this channel.
    pub login_modes: Vec<LoginMode>,
    pub capabilities: ChannelCapabilities,
    pub config_keys: Vec<ConfigKey>,
}

impl ChannelDescriptor {
    /// Convenience for tests and UIs that look one key up by name.
    pub fn config_key(&self, name: &str) -> Option<&ConfigKey> {
        self.config_keys.iter().find(|key| key.name == name)
    }
}
