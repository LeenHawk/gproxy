//! Log records (§8-D): raw downstream (client → proxy) and upstream
//! (proxy → provider) request logs. Both tables are append-only; retention is
//! deferred.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A raw downstream (client → proxy) request log entry (§8-D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownstreamRequest {
    pub id: i64,
    pub request_id: String,
    /// Unix seconds.
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub status: i64,
    #[serde(default)]
    pub headers_json: Option<Value>,
    pub body: Option<String>,
    /// Captured client-facing response body (§8-D); `None` when response-body
    /// logging is off or the response was not buffered.
    #[serde(default)]
    pub response_body: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Append input for a downstream request log entry (append-only; no id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownstreamRequestInput {
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub status: i64,
    #[serde(default)]
    pub headers_json: Option<Value>,
    pub body: Option<String>,
    /// Captured response body folded into the same INSERT for non-streaming
    /// responses (streaming backfills via `update_downstream_response`).
    #[serde(default)]
    pub response_body: Option<String>,
}

/// A raw upstream (proxy → provider) request log entry (§8-D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpstreamRequest {
    pub id: i64,
    pub request_id: String,
    /// Unix seconds.
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub url: String,
    pub method: String,
    pub status: i64,
    pub latency_ms: i64,
    #[serde(default)]
    pub headers_json: Option<Value>,
    pub body: Option<String>,
    /// Captured upstream (provider) response body (§8-D), post channel-decode /
    /// pre cross-protocol transform; `None` when logging is off.
    #[serde(default)]
    pub response_body: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Append input for an upstream request log entry (append-only; no id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpstreamRequestInput {
    pub request_id: String,
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub url: String,
    pub method: String,
    pub status: i64,
    pub latency_ms: i64,
    #[serde(default)]
    pub headers_json: Option<Value>,
    pub body: Option<String>,
    /// Captured upstream response body folded into the same INSERT for
    /// non-streaming responses (streaming backfills via `update_upstream_response`).
    #[serde(default)]
    pub response_body: Option<String>,
}
