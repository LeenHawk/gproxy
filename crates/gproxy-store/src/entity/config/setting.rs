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

    // Network
    /// Global proxy URL, overridden by Provider.proxy and then Credential.proxy.
    /// If all three are None, inherit_system_proxy controls system proxy fallback.
    #[sea_orm(column_type = "Text")]
    pub proxy: Option<String>,
    #[sea_orm(default_value = false)]
    pub inherit_system_proxy: bool,
    /// JSON array of browser origin strings.
    #[sea_orm(default_value = "[]")]
    pub cors_origins: Json,
    /// JSON array of trusted proxy addresses.
    #[sea_orm(default_value = "[]")]
    pub trusted_proxies: Json,

    // Execution
    #[sea_orm(default_value = 6)]
    pub max_attempts: u32,
    #[sea_orm(default_value = 1024)]
    pub max_in_flight: u32,
    /// Zero uses the existing unrestricted upload-concurrency setting.
    #[sea_orm(default_value = 0)]
    pub file_upload_max_in_flight: u32,
    /// Usage extraction, pricing and quota settlement.
    #[sea_orm(default_value = true)]
    pub enable_settlement: bool,
    /// Persist request usage records independently of settlement.
    #[sea_orm(default_value = true)]
    pub enable_usage: bool,

    // Token counting
    #[sea_orm(default_value = true)]
    pub enable_tokenizer_vocabs: bool,
    #[sea_orm(default_value = false)]
    pub enable_tokenizer_download: bool,
    /// Default custom vocabulary when the model has no vocabulary file selected.
    /// The tokenizer's fixed GPT selection still takes precedence.
    pub default_vocabulary_file_id: Option<String>,

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
    /// Name of a host-configured gproxy-file operator; None uses the host default.
    pub default_file_storage_name: Option<String>,
    /// None uses the application's retention default.
    pub retention_days: Option<u32>,
    /// Saved preference for database cleanup; does not describe S3 object storage.
    pub max_database_size_mb: Option<u64>,
    /// None follows the application's build channel.
    pub update_channel: Option<String>,
    #[sea_orm(default_value = true)]
    pub enable_auto_update_check: bool,

    // User portal
    #[sea_orm(default_value = true)]
    pub portal_recent_requests_enabled: bool,

    #[sea_orm(
        belongs_to,
        from = "default_vocabulary_file_id",
        to = "id",
        on_delete = "SetNull"
    )]
    pub default_vocabulary_file: BelongsTo<Option<crate::entity::resource::file_object::Entity>>,
}

impl ActiveModelBehavior for ActiveModel {}
