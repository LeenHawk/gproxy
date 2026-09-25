//! Minimal provider information usable without gateway configuration access.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialProviderDto {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub channel: String,
    pub enabled: bool,
    pub login_modes: Vec<gproxy_sdk::LoginMode>,
    pub capabilities: gproxy_sdk::ChannelCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialOwnerOptionDto {
    pub kind: String,
    pub id: String,
    pub name: String,
}
