use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureInput {
    pub request_id: String,
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub upstream_url: Option<String>,
    pub response_status: Option<u16>,
    pub request_body: Vec<u8>,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLogInput {
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
}
