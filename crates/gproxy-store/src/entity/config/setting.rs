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
    /// Client request headers allowed for every provider, unioned with each
    /// provider's allow-list and the channel's built-in headers.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
    pub allowed_headers: Json,
    /// Global connection profile. None uses the built-in reqwest/direct defaults.
    /// Credential then Provider selections override this entire profile.
    #[sea_orm(indexed)]
    pub connection_profile_id: Option<String>,
    /// Outbound proxy override. None inherits the parent scope; global None is direct.
    pub proxy: Option<Json>,
    /// JSON array of browser origin strings.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
    pub cors_origins: Json,
    /// JSON array of trusted proxy addresses.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
    pub trusted_proxies: Json,
    /// Mark the session cookie `Secure` whatever the request looked like. Off,
    /// it is `Secure` only when the request is known to be HTTPS, which behind
    /// a TLS-terminating proxy needs that proxy in `trusted_proxies`.
    #[sea_orm(default_value = false)]
    pub always_secure_cookie: bool,

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

    // Execution limits. Core derives its capability and codec limits from
    // these; there is no unlimited mode. Connect timeout lives on the profile.
    /// How long an upstream may take to start answering: its response head,
    /// or a converted non-streaming answer in full. Generous, because a
    /// high-effort reasoning request can be silent for many minutes; once a
    /// stream has started, `stream_idle_timeout_ms` bounds it instead.
    #[sea_orm(default_value = 1200000)]
    pub request_timeout_ms: u32,
    /// Maximum silence between stream progress events.
    #[sea_orm(default_value = 300000)]
    pub stream_idle_timeout_ms: u32,
    /// Every data-plane request body except a file upload, and the size a
    /// compressed request body may inflate to.
    #[sea_orm(default_value = 52428800)]
    pub max_request_body_bytes: i64,
    /// A file upload's body. Provider file APIs take hundreds of megabytes.
    #[sea_orm(default_value = 536870912)]
    pub max_upload_body_bytes: i64,
    /// Room for base64 images and video in a buffered answer.
    #[sea_orm(default_value = 268435456)]
    pub max_response_body_bytes: i64,
    /// One decoded SSE event, JSON array element, NDJSON record or JSON value.
    /// Large enough for a partial image sent as one base64 event.
    #[sea_orm(default_value = 33554432)]
    pub max_stream_event_bytes: i64,
    #[sea_orm(default_value = 33554432)]
    pub max_ws_frame_bytes: i64,
    #[sea_orm(default_value = 64)]
    pub max_multipart_parts: u32,

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
    #[sea_orm(default_value = false)]
    pub enable_downstream_log: bool,
    #[sea_orm(default_value = false)]
    pub enable_downstream_log_body: bool,
    #[sea_orm(default_value = false)]
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
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
    pub request_header_blacklist: Json,
    /// Additional response-header names to remove, as a JSON string array.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
    pub response_header_blacklist: Json,
    /// Additional query-parameter names to remove, as a JSON string array.
    #[sea_orm(default_expr = "sea_orm::sea_query::Expr::cust(\"('[]')\")")]
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
    pub update_source: Option<String>,
    /// None follows the CLI setting, which defaults to signature verification.
    pub update_verify_signature: Option<bool>,
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
