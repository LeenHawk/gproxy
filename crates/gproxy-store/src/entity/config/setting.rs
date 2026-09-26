//! Global runtime settings. The host reads the row with id = GLOBAL_SETTINGS_ID.
//! Database credentials, listener addresses and storage connections remain host configuration.

use sea_orm::entity::prelude::*;

pub const GLOBAL_SETTINGS_ID: i32 = 1;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "settings")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, default_value = 1)]
    pub id: i32,

    // Instance
    #[sea_orm(default_value = "default")]
    pub instance_name: String,

    /// OAuth client-ID ceiling as a JSON string array. None adds no restriction;
    /// [] denies all. Organization/team/user policies can only narrow it.
    pub oauth_client_allowlist: Option<Json>,

    // Network
    /// Global connection profile. None uses the built-in reqwest/direct defaults.
    /// Credential then Provider selections override this entire profile.
    #[sea_orm(indexed)]
    pub connection_profile_id: Option<String>,
    /// Outbound proxy override. None inherits the parent scope; global None is direct.
    pub proxy: Option<Json>,
    /// JSON array of browser origin strings.
    #[sea_orm(default_value = "[]")]
    pub cors_origins: Json,
    /// JSON array of trusted proxy addresses.
    #[sea_orm(default_value = "[]")]
    pub trusted_proxies: Json,

    // Execution
    #[sea_orm(default_value = 6)]
    pub max_attempts: u32,
    /// Usage extraction, pricing and quota settlement.
    #[sea_orm(default_value = true)]
    pub enable_settlement: bool,
    /// Persist request usage records independently of settlement.
    #[sea_orm(default_value = true)]
    pub enable_usage: bool,
    /// Incremented in the same transaction as any execution-configuration
    /// write. Core publishes snapshots monotonically by this value.
    #[sea_orm(default_value = 0)]
    pub config_revision: i64,

    // Token counting
    #[sea_orm(default_value = true)]
    pub enable_tokenizer_vocabs: bool,
    #[sea_orm(default_value = false)]
    pub enable_tokenizer_download: bool,
    /// Default custom vocabulary when the model has no vocabulary file selected.
    /// The tokenizer's fixed GPT selection still takes precedence.
    pub default_vocabulary_file_id: Option<String>,
    /// Host-sealed access token for the vocabulary source (a Hugging Face
    /// token for gated repositories). None downloads anonymously.
    pub tokenizer_auth_token: Option<Vec<u8>>,

    // Logging and capture
    #[sea_orm(default_value = true)]
    pub enable_downstream_log: bool,
    #[sea_orm(default_value = false)]
    pub enable_downstream_log_body: bool,
    #[sea_orm(default_value = true)]
    pub enable_upstream_log: bool,
    #[sea_orm(default_value = false)]
    pub enable_upstream_log_body: bool,
    #[sea_orm(default_value = false)]
    pub disable_log_redaction: bool,
    #[sea_orm(default_value = true)]
    pub enable_tracing: bool,
    #[sea_orm(default_value = "info")]
    pub log_level: String,
    #[sea_orm(default_value = "text")]
    pub log_format: String,
    /// Additional request-header names to remove, as a JSON string array.
    #[sea_orm(default_value = "[]")]
    pub request_header_blacklist: Json,
    /// Additional response-header names to remove, as a JSON string array.
    #[sea_orm(default_value = "[]")]
    pub response_header_blacklist: Json,
    /// Additional query-parameter names to remove, as a JSON string array.
    #[sea_orm(default_value = "[]")]
    pub query_parameter_blacklist: Json,

    // Files and maintenance
    /// Completed request history retention; None disables age-based cleanup.
    pub retention_days: Option<u32>,
    /// Upstream quota observation log (`credential_quota_cycles`) retention;
    /// None keeps every observation. Separate from `retention_days` because
    /// the log feeds per-cycle cost analysis long after request bodies are
    /// gone. The cycles themselves (`credential_cycles`) are never pruned.
    #[sea_orm(default_value = 90)]
    pub quota_observation_retention_days: Option<u32>,
    /// SQLite history budget in MiB; None or zero disables size-based cleanup.
    pub max_database_size_mb: Option<i64>,
    /// None follows the application's build channel.
    pub update_channel: Option<String>,
    #[sea_orm(default_value = true)]
    pub enable_auto_update_check: bool,

    // User portal
    #[sea_orm(default_value = true)]
    pub portal_recent_requests_enabled: bool,

    #[sea_orm(
        belongs_to,
        from = "connection_profile_id",
        to = "id",
        on_delete = "Restrict"
    )]
    pub connection_profile: BelongsTo<Option<super::connection_profile::Entity>>,
    #[sea_orm(
        belongs_to,
        from = "default_vocabulary_file_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub default_vocabulary_file: BelongsTo<Option<crate::entity::resource::file_object::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
