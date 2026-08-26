use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub channel: String,
    pub settings: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: i64,
    pub name: String,
    pub channel: String,
    pub settings: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialEnvelope {
    pub ciphertext: Vec<u8>,
    pub wrapped_key: Vec<u8>,
    pub payload_nonce: Vec<u8>,
    pub key_nonce: Vec<u8>,
}

impl std::fmt::Debug for CredentialEnvelope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialEnvelope(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialInput {
    pub provider_id: i64,
    pub label: Option<String>,
    pub envelope: CredentialEnvelope,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub id: i64,
    pub provider_id: i64,
    pub channel: String,
    pub label: Option<String>,
    pub envelope: CredentialEnvelope,
    pub version: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialMetaRecord {
    pub id: i64,
    pub provider_id: i64,
    pub version: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialAdminRecord {
    pub id: i64,
    pub provider_id: i64,
    pub label: Option<String>,
    pub version: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialUpdateInput {
    pub provider_id: i64,
    pub label: Option<String>,
    pub envelope: Option<CredentialEnvelope>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteInput {
    pub name: String,
    pub max_attempts: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRecord {
    pub id: i64,
    pub name: String,
    pub max_attempts: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMemberInput {
    pub route_id: i64,
    pub provider_id: i64,
    pub credential_id: Option<i64>,
    pub upstream_model: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMemberRecord {
    pub id: i64,
    pub route_id: i64,
    pub provider_id: i64,
    pub credential_id: Option<i64>,
    pub upstream_model: String,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasInput {
    pub alias: String,
    pub target: String,
    pub provider_id: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasRecord {
    pub id: i64,
    pub alias: String,
    pub target: String,
    pub provider_id: Option<i64>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposedModelInput {
    pub name: String,
    pub route_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExposedModelRecord {
    pub id: i64,
    pub name: String,
    pub route_id: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRuleInput {
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    pub tiers: Option<Value>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRuleRecord {
    pub id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    pub tiers: Option<Value>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRateInput {
    pub rule_id: i64,
    pub metric: String,
    pub unit_size: u64,
    pub price: rust_decimal::Decimal,
    pub conditions: Option<Value>,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRateRecord {
    pub id: i64,
    pub rule_id: i64,
    pub metric: String,
    pub unit_size: u64,
    pub price: rust_decimal::Decimal,
    pub conditions: Option<Value>,
    pub priority: i64,
}
