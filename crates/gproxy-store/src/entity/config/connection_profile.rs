//! Reusable HTTP transport settings; proxy routing is configured by scope. No profile inheritance or version.
//! The host validates these fields with gproxy-client before saving/using them.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "connection_profiles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique)]
    pub name: String,
    #[sea_orm(default_value = "reqwest")]
    pub backend: Backend,
    /// Optional gproxy-client EmulationConfig object: profile, platform,
    /// http2, headers. Only valid with Wreq; None uses the backend's normal TLS.
    pub emulation: Option<Json>,
    /// Automatic response decompression; false preserves encoded bytes/headers.
    #[sea_orm(default_value = false)]
    pub gzip: bool,
    #[sea_orm(default_value = false)]
    pub brotli: bool,
    #[sea_orm(default_value = false)]
    pub deflate: bool,
    #[sea_orm(default_value = false)]
    pub zstd: bool,
    /// Zero disables following redirects; otherwise maximum redirect hops.
    #[sea_orm(default_value = 0)]
    pub redirect_max_hops: u32,
    #[sea_orm(default_value = "never")]
    pub retry: RetryPolicy,
    #[sea_orm(default_value = 10000)]
    pub connect_timeout_ms: u32,
    /// Zero disables retention of idle connections.
    #[sea_orm(default_value = 90000)]
    pub pool_idle_timeout_ms: u32,
    /// Idle connections per host, not the request concurrency limit.
    #[sea_orm(default_value = 32)]
    pub pool_max_idle_per_host: u32,
    pub created_at_ms: i64,
    #[sea_orm(has_many)]
    pub settings: HasMany<super::setting::Entity>,
    #[sea_orm(has_many)]
    pub providers: HasMany<crate::entity::upstream::provider::Entity>,
    #[sea_orm(has_many)]
    pub credentials: HasMany<crate::entity::upstream::credential::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// Matches gproxy-client RetryPolicy. Default uses native protocol-NACK retries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum RetryPolicy {
    #[sea_orm(string_value = "never")]
    Never,
    #[sea_orm(string_value = "default")]
    Default,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum Backend {
    #[sea_orm(string_value = "reqwest")]
    Reqwest,
    #[sea_orm(string_value = "wreq")]
    Wreq,
    /// reqwest 0.12 over the platform's native TLS (gproxy-client feature
    /// `reqwest-native`); the Codex CLI's HTTP stack.
    #[sea_orm(string_value = "reqwest_native")]
    ReqwestNative,
}
