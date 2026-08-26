use serde::{Deserialize, Serialize};

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
