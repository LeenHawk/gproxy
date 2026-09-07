use serde::{Deserialize, Serialize};

pub const ENABLE_DOWNSTREAM_LOG: &str = "enable_downstream_log";
pub const ENABLE_DOWNSTREAM_LOG_BODY: &str = "enable_downstream_log_body";
pub const ENABLE_UPSTREAM_LOG: &str = "enable_upstream_log";
pub const ENABLE_UPSTREAM_LOG_BODY: &str = "enable_upstream_log_body";
pub const DISABLE_LOG_REDACTION: &str = "disable_log_redaction";
pub const RETENTION_DAYS: &str = "retention_days";
pub const MAX_DATABASE_SIZE_MB: &str = "max_database_size_mb";
pub const PROXY: &str = "proxy";
pub const ENABLE_USAGE: &str = "enable_usage";
pub const ENABLE_TOKENIZER_VOCABS: &str = "enable_tokenizer_vocabs";
pub const ENABLE_TOKENIZER_DOWNLOAD: &str = "enable_tokenizer_download";
pub const DEFAULT_TOKENIZER_VOCAB: &str = "default_tokenizer_vocab";
pub const FILE_UPLOAD_MAX_IN_FLIGHT: &str = "file_upload_max_in_flight";
pub const INSTANCE_NAME: &str = "instance_name";
pub const INHERIT_SYSTEM_PROXY: &str = "inherit_system_proxy";
pub const TRAFFIC_BLACKLIST: &str = "traffic_blacklist";
pub const UPDATE_CHANNEL: &str = "update_channel";
pub const ENABLE_AUTO_UPDATE_CHECK: &str = "enable_auto_update_check";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureInput {
    pub request_id: String,
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub upstream_url: Option<String>,
    pub request_method: Option<String>,
    pub request_headers: Option<serde_json::Value>,
    pub response_status: Option<u16>,
    pub response_headers: Option<serde_json::Value>,
    pub request_body: Option<Vec<u8>>,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLogInput {
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub client_ip: Option<String>,
    pub request_headers: Option<serde_json::Value>,
    pub request_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestLogCompletion {
    pub request_id: String,
    pub response_status: u16,
    pub error_kind: Option<String>,
    pub response_headers: Option<serde_json::Value>,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLogImportInput {
    pub request: RequestLogInput,
    pub response_status: Option<u16>,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogQuery {
    pub start: i64,
    pub end: i64,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub status: Option<u16>,
    pub request_id: Option<String>,
    pub cursor: Option<i64>,
    pub limit: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLogSummary {
    pub id: i64,
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub response_status: Option<u16>,
    pub error_kind: Option<String>,
    pub client_ip: Option<String>,
    pub duration_ms: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPage {
    pub items: Vec<RequestLogSummary>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestLogRecord {
    pub id: i64,
    pub input: RequestLogInput,
    pub response_status: Option<u16>,
    pub error_kind: Option<String>,
    pub response_headers: Option<serde_json::Value>,
    pub response_body: Option<Vec<u8>>,
    pub duration_ms: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WireLogRecord {
    pub id: i64,
    pub input: CaptureInput,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogDetail {
    pub downstream: RequestLogRecord,
    pub upstream: Vec<WireLogRecord>,
}
