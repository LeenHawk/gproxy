//! Outbound connection profiles: one complete client configuration per row.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::config::connection_profile as profile;

/// A profile as management sees it. `proxy_url` is returned so a console can
/// edit it; it may carry proxy credentials, which is why the management API
/// is an operator surface and its responses are not for a caller's eyes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfileDto {
    pub id: String,
    pub name: String,
    /// `reqwest`, `wreq` or `reqwest_native`.
    pub backend: String,
    /// `direct`, `system` or `explicit`.
    pub proxy_mode: String,
    pub proxy_url: Option<String>,
    /// A `gproxy-client` emulation object, or null. Only meaningful with `wreq`.
    pub emulation: Option<Value>,
    pub gzip: bool,
    pub brotli: bool,
    pub deflate: bool,
    pub zstd: bool,
    pub redirect_max_hops: u32,
    /// `never` or `default`.
    pub retry: String,
    pub connect_timeout_ms: u32,
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
            proxy_mode: proxy_mode_name(row.proxy_mode).to_owned(),
            proxy_url: row.proxy_url,
            emulation: row.emulation,
            gzip: row.gzip,
            brotli: row.brotli,
            deflate: row.deflate,
            zstd: row.zstd,
            redirect_max_hops: row.redirect_max_hops,
            retry: retry_name(row.retry).to_owned(),
            connect_timeout_ms: row.connect_timeout_ms,
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

pub(crate) fn proxy_mode_name(mode: profile::ProxyMode) -> &'static str {
    match mode {
        profile::ProxyMode::Direct => "direct",
        profile::ProxyMode::System => "system",
        profile::ProxyMode::Explicit => "explicit",
    }
}

pub(crate) fn retry_name(retry: profile::RetryPolicy) -> &'static str {
    match retry {
        profile::RetryPolicy::Never => "never",
        profile::RetryPolicy::Default => "default",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfileWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub proxy_mode: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
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
    pub connect_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_idle_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_max_idle_per_host: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfilePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub proxy_mode: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub proxy_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
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
    pub connect_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_idle_timeout_ms: Option<u32>,
    #[serde(default)]
    pub pool_max_idle_per_host: Option<u32>,
}
