use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::TlsFingerprintDto;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ProviderDto {
    pub id: i64,
    pub name: String,
    pub channel: String,
    #[ts(type = "unknown")]
    pub settings: Value,
    pub tls_fingerprint: Option<TlsFingerprintDto>,
    #[ts(type = "unknown | null")]
    pub invalid_tls_fingerprint: Option<Value>,
    pub tls_fingerprint_error: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ProviderWriteRequest {
    pub name: String,
    pub channel: String,
    #[ts(type = "unknown")]
    pub settings: Value,
    pub tls_fingerprint: Option<TlsFingerprintDto>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum CredentialHealthDto {
    Disabled,
    Unknown,
    Healthy,
    Degraded,
    Dead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CredentialDto {
    pub id: i64,
    pub provider_id: i64,
    pub label: Option<String>,
    pub version: u64,
    pub enabled: bool,
    pub health: CredentialHealthDto,
    pub health_observed_at: Option<i64>,
    pub health_response_status: Option<u16>,
    pub health_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CredentialWriteRequest {
    pub provider_id: i64,
    pub label: Option<String>,
    #[ts(type = "unknown | null")]
    pub secret: Option<Value>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RouteDto {
    pub id: i64,
    pub name: String,
    pub max_attempts: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RouteWriteRequest {
    pub name: String,
    pub max_attempts: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RouteMemberDto {
    pub id: i64,
    pub route_id: i64,
    pub provider_id: i64,
    pub credential_id: Option<i64>,
    pub upstream_model: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RouteMemberWriteRequest {
    pub route_id: i64,
    pub provider_id: i64,
    pub credential_id: Option<i64>,
    pub upstream_model: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AliasDto {
    pub id: i64,
    pub alias: String,
    pub target: String,
    pub provider_id: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AliasWriteRequest {
    pub alias: String,
    pub target: String,
    pub provider_id: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModelAliasDto {
    pub id: i64,
    pub name: String,
    pub route_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModelAliasWriteRequest {
    pub name: String,
    pub route_id: i64,
    pub enabled: bool,
}
