use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use super::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpstreamRequestQuery {
    #[serde(default)]
    pub trace_id: Scope<i64>,
    pub provider_id: Scope<i64>,
    pub credential_id: Scope<i64>,
    pub request_url_contains: Option<String>,
    pub from_unix_ms: Option<i64>,
    pub to_unix_ms: Option<i64>,
    pub cursor_at_unix_ms: Option<i64>,
    pub cursor_trace_id: Option<i64>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub include_body: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamRequestQueryRow {
    pub trace_id: i64,
    pub downstream_trace_id: Option<i64>,
    pub at: OffsetDateTime,
    pub internal: bool,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub request_method: String,
    pub request_headers_json: Value,
    pub request_url: Option<String>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<i32>,
    pub response_headers_json: Value,
    pub response_body: Option<Vec<u8>>,
    pub initial_latency_ms: Option<i64>,
    pub total_latency_ms: Option<i64>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DownstreamRequestQuery {
    #[serde(default)]
    pub trace_id: Scope<i64>,
    pub user_id: Scope<i64>,
    pub user_key_id: Scope<i64>,
    pub request_path_contains: Option<String>,
    pub from_unix_ms: Option<i64>,
    pub to_unix_ms: Option<i64>,
    pub cursor_at_unix_ms: Option<i64>,
    pub cursor_trace_id: Option<i64>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub include_body: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownstreamRequestQueryRow {
    pub trace_id: i64,
    pub at: OffsetDateTime,
    pub internal: bool,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub request_method: String,
    pub request_headers_json: Value,
    pub request_path: String,
    pub request_query: Option<String>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<i32>,
    pub response_headers_json: Value,
    pub response_body: Option<Vec<u8>>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RequestQueryCount {
    pub count: u64,
}
