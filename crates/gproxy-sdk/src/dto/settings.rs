//! The single settings row, split into the two groups a console edits
//! separately. There is one durable row underneath both.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::config::setting;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SettingsDto {
    pub instance: InstanceSettingsDto,
    pub logging: LoggingSettingsDto,
}

impl From<setting::Model> for SettingsDto {
    fn from(row: setting::Model) -> Self {
        Self {
            logging: LoggingSettingsDto {
                enable_downstream_log: row.enable_downstream_log,
                enable_downstream_log_body: row.enable_downstream_log_body,
                enable_upstream_log: row.enable_upstream_log,
                enable_upstream_log_body: row.enable_upstream_log_body,
                disable_log_redaction: row.disable_log_redaction,
                enable_tracing: row.enable_tracing,
                log_level: row.log_level,
                log_format: row.log_format,
                request_header_blacklist: row.request_header_blacklist,
                response_header_blacklist: row.response_header_blacklist,
                query_parameter_blacklist: row.query_parameter_blacklist,
            },
            instance: InstanceSettingsDto {
                instance_name: row.instance_name,
                oauth_client_allowlist: row.oauth_client_allowlist,
                allowed_headers: row.allowed_headers,
                connection_profile_id: row.connection_profile_id,
                proxy: row.proxy,
                cors_origins: row.cors_origins,
                trusted_proxies: row.trusted_proxies,
                always_secure_cookie: row.always_secure_cookie,
                max_attempts: row.max_attempts,
                enable_settlement: row.enable_settlement,
                enable_usage: row.enable_usage,
                config_revision: row.config_revision,
                request_timeout_ms: row.request_timeout_ms,
                stream_idle_timeout_ms: row.stream_idle_timeout_ms,
                max_request_body_bytes: row.max_request_body_bytes,
                max_upload_body_bytes: row.max_upload_body_bytes,
                max_response_body_bytes: row.max_response_body_bytes,
                max_stream_event_bytes: row.max_stream_event_bytes,
                max_ws_frame_bytes: row.max_ws_frame_bytes,
                max_multipart_parts: row.max_multipart_parts,
                enable_tokenizer_vocabs: row.enable_tokenizer_vocabs,
                enable_tokenizer_download: row.enable_tokenizer_download,
                default_vocabulary_file_id: row.default_vocabulary_file_id,
                has_tokenizer_auth_token: row
                    .tokenizer_auth_token
                    .is_some_and(|token| !token.is_empty()),
                retention_days: row.retention_days,
                quota_observation_retention_days: row.quota_observation_retention_days,
                max_database_size_mb: row.max_database_size_mb,
                update_channel: row.update_channel,
                update_source: row.update_source,
                enable_auto_update_check: row.enable_auto_update_check,
                portal_recent_requests_enabled: row.portal_recent_requests_enabled,
            },
        }
    }
}

/// Identity, network, execution limits and maintenance. `config_revision` is
/// read-only: it is the write path's own counter.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct InstanceSettingsDto {
    pub instance_name: String,
    #[serde(default = "empty_header_list")]
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub allowed_headers: Value,
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub oauth_client_allowlist: Option<Value>,
    pub connection_profile_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Value>,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub cors_origins: Value,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub trusted_proxies: Value,
    /// Absent in an export from a build without it, which reads as off.
    #[serde(default)]
    pub always_secure_cookie: bool,
    pub max_attempts: u32,
    pub enable_settlement: bool,
    pub enable_usage: bool,
    pub config_revision: i64,
    // Execution limits. Absent in an export from a build that had none, which
    // reads as the defaults.
    #[serde(default = "limit_defaults::request_timeout_ms")]
    pub request_timeout_ms: u32,
    #[serde(default = "limit_defaults::stream_idle_timeout_ms")]
    pub stream_idle_timeout_ms: u32,
    #[serde(default = "limit_defaults::max_request_body_bytes")]
    pub max_request_body_bytes: i64,
    #[serde(default = "limit_defaults::max_upload_body_bytes")]
    pub max_upload_body_bytes: i64,
    #[serde(default = "limit_defaults::max_response_body_bytes")]
    pub max_response_body_bytes: i64,
    #[serde(default = "limit_defaults::max_stream_event_bytes")]
    pub max_stream_event_bytes: i64,
    #[serde(default = "limit_defaults::max_ws_frame_bytes")]
    pub max_ws_frame_bytes: i64,
    #[serde(default = "limit_defaults::max_multipart_parts")]
    pub max_multipart_parts: u32,
    pub enable_tokenizer_vocabs: bool,
    pub enable_tokenizer_download: bool,
    pub default_vocabulary_file_id: Option<String>,
    /// The vocabulary source token is sealed like a credential secret; only
    /// its presence is reported.
    pub has_tokenizer_auth_token: bool,
    pub retention_days: Option<u32>,
    /// Days of upstream quota observations to keep; None keeps them all.
    /// Cycles are never pruned. Absent in an older export, which reads as
    /// the 90-day default.
    #[serde(default = "default_observation_retention")]
    pub quota_observation_retention_days: Option<u32>,
    pub max_database_size_mb: Option<i64>,
    pub update_channel: Option<String>,
    pub update_source: Option<String>,
    pub enable_auto_update_check: bool,
    pub portal_recent_requests_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LoggingSettingsDto {
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub disable_log_redaction: bool,
    pub enable_tracing: bool,
    pub log_level: String,
    pub log_format: String,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub request_header_blacklist: Value,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub response_header_blacklist: Value,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub query_parameter_blacklist: Value,
}

/// Both groups are optional, and so is every field inside them: a patch names
/// only what changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SettingsPatch {
    #[serde(default)]
    pub instance: Option<InstanceSettingsPatch>,
    #[serde(default)]
    pub logging: Option<LoggingSettingsPatch>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct InstanceSettingsPatch {
    #[serde(default)]
    pub instance_name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub allowed_headers: Option<Value>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub oauth_client_allowlist: Option<Option<Value>>,
    #[serde(default, deserialize_with = "double_option")]
    pub connection_profile_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Option<Value>>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub cors_origins: Option<Value>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub trusted_proxies: Option<Value>,
    #[serde(default)]
    pub always_secure_cookie: Option<bool>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub enable_settlement: Option<bool>,
    #[serde(default)]
    pub enable_usage: Option<bool>,
    #[serde(default)]
    pub request_timeout_ms: Option<u32>,
    #[serde(default)]
    pub stream_idle_timeout_ms: Option<u32>,
    #[serde(default)]
    pub max_request_body_bytes: Option<i64>,
    #[serde(default)]
    pub max_upload_body_bytes: Option<i64>,
    #[serde(default)]
    pub max_response_body_bytes: Option<i64>,
    #[serde(default)]
    pub max_stream_event_bytes: Option<i64>,
    #[serde(default)]
    pub max_ws_frame_bytes: Option<i64>,
    #[serde(default)]
    pub max_multipart_parts: Option<u32>,
    #[serde(default)]
    pub enable_tokenizer_vocabs: Option<bool>,
    #[serde(default)]
    pub enable_tokenizer_download: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub default_vocabulary_file_id: Option<Option<String>>,
    /// Plaintext on the way in, sealed before it reaches the row. `null`
    /// removes it and downloads anonymously again.
    #[serde(default, deserialize_with = "double_option")]
    pub tokenizer_auth_token: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub retention_days: Option<Option<u32>>,
    /// `null` keeps every observation.
    #[serde(default, deserialize_with = "double_option")]
    pub quota_observation_retention_days: Option<Option<u32>>,
    #[serde(default, deserialize_with = "double_option")]
    pub max_database_size_mb: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub update_channel: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub update_source: Option<Option<String>>,
    #[serde(default)]
    pub enable_auto_update_check: Option<bool>,
    #[serde(default)]
    pub portal_recent_requests_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct LoggingSettingsPatch {
    #[serde(default)]
    pub enable_downstream_log: Option<bool>,
    #[serde(default)]
    pub enable_downstream_log_body: Option<bool>,
    #[serde(default)]
    pub enable_upstream_log: Option<bool>,
    #[serde(default)]
    pub enable_upstream_log_body: Option<bool>,
    #[serde(default)]
    pub disable_log_redaction: Option<bool>,
    #[serde(default)]
    pub enable_tracing: Option<bool>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub log_format: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub request_header_blacklist: Option<Value>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub response_header_blacklist: Option<Value>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub query_parameter_blacklist: Option<Value>,
}

/// Serde defaults for the execution limits, taken from core's own so the two
/// cannot disagree.
mod limit_defaults {
    use gproxy_core::ExecutionLimits;

    fn limits() -> ExecutionLimits {
        ExecutionLimits::default()
    }
    fn millis(duration: std::time::Duration) -> u32 {
        u32::try_from(duration.as_millis()).unwrap_or(u32::MAX)
    }
    fn bytes(value: u64) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }
    pub(super) fn request_timeout_ms() -> u32 {
        millis(limits().request_timeout)
    }
    pub(super) fn stream_idle_timeout_ms() -> u32 {
        millis(limits().stream_idle_timeout)
    }
    pub(super) fn max_request_body_bytes() -> i64 {
        bytes(limits().max_request_body_bytes)
    }
    pub(super) fn max_upload_body_bytes() -> i64 {
        bytes(limits().max_upload_body_bytes)
    }
    pub(super) fn max_response_body_bytes() -> i64 {
        bytes(limits().max_response_body_bytes)
    }
    pub(super) fn max_stream_event_bytes() -> i64 {
        bytes(limits().max_stream_event_bytes)
    }
    pub(super) fn max_ws_frame_bytes() -> i64 {
        bytes(limits().max_ws_frame_bytes)
    }
    pub(super) fn max_multipart_parts() -> u32 {
        u32::try_from(limits().max_multipart_parts).unwrap_or(u32::MAX)
    }
}

fn default_observation_retention() -> Option<u32> {
    Some(90)
}

fn empty_header_list() -> Value {
    serde_json::json!([])
}
