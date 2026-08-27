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
