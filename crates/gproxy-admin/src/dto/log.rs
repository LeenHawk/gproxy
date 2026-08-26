use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogQueryDto {
    pub start: i64,
    pub end: i64,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub status: Option<u16>,
    pub request_id: Option<String>,
    pub cursor: Option<i64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogListItemDto {
    pub id: i64,
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub response_status: Option<u16>,
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogPageDto {
    pub items: Vec<LogListItemDto>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DownstreamLogDto {
    pub id: i64,
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    #[ts(type = "Record<string, string> | null")]
    pub request_headers: Option<Value>,
    pub request_body: Option<String>,
    pub response_status: Option<u16>,
    pub error_kind: Option<String>,
    #[ts(type = "Record<string, string> | null")]
    pub response_headers: Option<Value>,
    pub response_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WireLogDto {
    pub id: i64,
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub upstream_url: Option<String>,
    pub request_method: Option<String>,
    #[ts(type = "Record<string, string> | null")]
    pub request_headers: Option<Value>,
    pub request_body: Option<String>,
    pub response_status: Option<u16>,
    #[ts(type = "Record<string, string> | null")]
    pub response_headers: Option<Value>,
    pub response_body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LogDetailDto {
    pub downstream: DownstreamLogDto,
    pub upstream: Vec<WireLogDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogSettingsDto {
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub disable_log_redaction: bool,
    pub retention_days: Option<u64>,
    pub max_database_size_mb: Option<u64>,
    pub body_capture_allowed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LogSettingsUpdateDto {
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub disable_log_redaction: bool,
}
