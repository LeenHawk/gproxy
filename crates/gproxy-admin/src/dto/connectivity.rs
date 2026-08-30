use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ConnectivityScopeDto {
    Global,
    Proxy,
    Provider,
    Credential,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConnectivityTestRequest {
    pub scope: ConnectivityScopeDto,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub proxy_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ConnectivityProxySourceDto {
    Proxy,
    Credential,
    Provider,
    Global,
    System,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConnectivityProbeDto {
    pub ip: String,
    pub colo: Option<String>,
    pub location: Option<String>,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConnectivityTestResponse {
    pub ok: bool,
    pub ipv4: Option<ConnectivityProbeDto>,
    pub ipv6: Option<ConnectivityProbeDto>,
    pub latency_ms: u64,
    pub proxy_source: ConnectivityProxySourceDto,
    pub error_code: Option<String>,
    pub message: Option<String>,
}

/// Send one small completion through the funnel and report what came back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ModelTestRequest {
    pub provider_id: i64,
    pub model_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ModelTestResponse {
    pub ok: bool,
    pub status: u16,
    pub latency_ms: u64,
    /// Which key paid for it. A test that costs money says whose budget it came from.
    pub key_prefix: String,
    pub reply: Option<String>,
    pub message: Option<String>,
}
