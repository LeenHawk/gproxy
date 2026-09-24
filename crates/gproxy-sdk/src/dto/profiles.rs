//! Outbound connection profiles: one complete client configuration per row.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::config::connection_profile as profile;

/// A reusable HTTP transport configuration, independent of outbound proxy selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConnectionProfileDto {
    pub id: String,
    pub name: String,
    /// `reqwest`, `wreq` or `reqwest_native`.
    pub backend: String,
    /// A `gproxy-client` emulation object, or null. wreq applies the full identity; other clients apply supported custom fields.
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub emulation: Option<Value>,
    pub gzip: bool,
    pub brotli: bool,
    pub deflate: bool,
    pub zstd: bool,
    pub redirect_max_hops: u32,
    /// `never` or `default`.
    pub retry: String,
    pub pool_idle_timeout_ms: u32,
    pub pool_max_idle_per_host: u32,
    pub created_at_ms: i64,
}

impl From<profile::Model> for ConnectionProfileDto {
    fn from(row: profile::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            backend: backend_name(row.backend).to_owned(),
            emulation: row.emulation,
            gzip: row.gzip,
            brotli: row.brotli,
            deflate: row.deflate,
            zstd: row.zstd,
            redirect_max_hops: row.redirect_max_hops,
            retry: retry_name(row.retry).to_owned(),

            pool_idle_timeout_ms: row.pool_idle_timeout_ms,
            pool_max_idle_per_host: row.pool_max_idle_per_host,
            created_at_ms: row.created_at_ms,
        }
    }
}

pub(crate) fn backend_name(backend: profile::Backend) -> &'static str {
    match backend {
        profile::Backend::Reqwest => "reqwest",
        profile::Backend::Wreq => "wreq",
        profile::Backend::ReqwestNative => "reqwest_native",
    }
}

pub(crate) fn retry_name(retry: profile::RetryPolicy) -> &'static str {
    match retry {
        profile::RetryPolicy::Never => "never",
        profile::RetryPolicy::Default => "default",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConnectionProfileWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub emulation: Option<Value>,
    #[serde(default)]
    pub gzip: Option<bool>,
    #[serde(default)]
    pub brotli: Option<bool>,
    #[serde(default)]
    pub deflate: Option<bool>,
    #[serde(default)]
    pub zstd: Option<bool>,
    #[serde(default)]
    pub redirect_max_hops: Option<u32>,
    #[serde(default)]
    pub retry: Option<String>,
    #[serde(default)]
    pub pool_idle_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_max_idle_per_host: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConnectionProfilePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub emulation: Option<Option<Value>>,
    #[serde(default)]
    pub gzip: Option<bool>,
    #[serde(default)]
    pub brotli: Option<bool>,
    #[serde(default)]
    pub deflate: Option<bool>,
    #[serde(default)]
    pub zstd: Option<bool>,
    #[serde(default)]
    pub redirect_max_hops: Option<u32>,
    #[serde(default)]
    pub retry: Option<String>,
    #[serde(default)]
    pub pool_idle_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_max_idle_per_host: Option<u32>,
}
