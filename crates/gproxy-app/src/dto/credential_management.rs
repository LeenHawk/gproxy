//! Minimal provider information usable without gateway configuration access.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CredentialProviderDto {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub channel: String,
    pub enabled: bool,
    pub credential_kinds: Vec<String>,
    pub login_modes: Vec<gproxy_sdk::LoginMode>,
    /// Named choices inside `login_modes`; empty means each mode is one choice.
    #[serde(default)]
    pub login_variants: Vec<CredentialLoginVariantDto>,
    pub capabilities: gproxy_sdk::ChannelCapabilities,
}

/// One `LoginVariant` of the provider's channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CredentialLoginVariantDto {
    pub id: String,
    pub label: String,
    pub mode: gproxy_sdk::LoginMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CredentialOwnerOptionDto {
    pub kind: String,
    pub id: String,
    pub name: String,
}
