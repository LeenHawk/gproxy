use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInput {
    pub name: String,
    pub label: Option<String>,
    pub channel: String,
    pub settings: Value,
    pub credential_strategy: String,
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRecord {
    pub id: i64,
    pub name: String,
    pub label: Option<String>,
    pub channel: String,
    pub settings: Value,
    pub credential_strategy: String,
    pub proxy_url: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MasterKeyFingerprint {
    Missing,
    Plaintext,
    Sealed(String),
}

#[derive(Clone, PartialEq, Eq)]
pub struct StoredSecret {
    pub id: i64,
    pub envelope: CredentialEnvelope,
}

impl std::fmt::Debug for StoredSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StoredSecret")
            .field("id", &self.id)
            .field("envelope", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretInventory {
    pub fingerprint: MasterKeyFingerprint,
    pub credentials: Vec<StoredSecret>,
    pub user_keys: Vec<StoredSecret>,
    pub tokenizer_auth: Vec<super::TokenizerAuthSecret>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialInput {
    pub provider_id: i64,
    pub label: Option<String>,
    pub kind: String,
    pub envelope: CredentialEnvelope,
    pub enabled: bool,
    pub weight: u32,
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub id: i64,
    pub provider_id: i64,
    pub channel: String,
    pub label: Option<String>,
    pub kind: String,
    pub envelope: CredentialEnvelope,
    pub version: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialMetaRecord {
    pub id: i64,
    pub provider_id: i64,
    pub kind: String,
    pub version: u64,
    pub enabled: bool,
    pub weight: u32,
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialAdminRecord {
    pub id: i64,
    pub provider_id: i64,
    pub label: Option<String>,
    pub kind: String,
    pub version: u64,
    pub enabled: bool,
    pub weight: u32,
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialUpdateInput {
    pub provider_id: i64,
    pub label: Option<String>,
    pub kind: String,
    pub envelope: Option<CredentialEnvelope>,
    pub enabled: bool,
    pub weight: u32,
    pub rpm_limit: Option<u32>,
    pub tpm_limit: Option<u64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
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
    pub tier: u32,
    pub weight: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMemberRecord {
    pub id: i64,
    pub route_id: i64,
    pub provider_id: i64,
    pub credential_id: Option<i64>,
    pub upstream_model: String,
    pub tier: u32,
    pub weight: u32,
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

/// What one provider supports for one upstream model. The exposed catalogue is the
/// conservative fold of these across a route's members, never a value typed by hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModelInput {
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub variants: Option<Value>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
    #[serde(default)]
    pub metadata: gproxy_core::ModelMetadata,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModelRecord {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub variants: Option<Value>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
    #[serde(default)]
    pub metadata: gproxy_core::ModelMetadata,
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
