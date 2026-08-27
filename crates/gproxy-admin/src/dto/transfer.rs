use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum SecretExportDto {
    Omitted,
    Included,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[ts(tag = "mode", rename_all = "snake_case")]
pub enum ExportSourceKeyDto {
    Plaintext,
    Sealed { fingerprint: String },
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CredentialEnvelopeDto {
    pub ciphertext: Vec<u8>,
    pub wrapped_key: Vec<u8>,
    pub payload_nonce: Vec<u8>,
    pub key_nonce: Vec<u8>,
}

impl std::fmt::Debug for CredentialEnvelopeDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialEnvelopeDto(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ExportCredentialDto {
    pub config: CredentialDto,
    pub secret: Option<CredentialEnvelopeDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExportUserKeyDto {
    pub config: UserKeyDto,
    pub digest: Vec<u8>,
    pub digest_version: u32,
    pub secret: Option<CredentialEnvelopeDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ConfigurationDataDto {
    pub organizations: Vec<OrganizationDto>,
    pub teams: Vec<TeamDto>,
    pub users: Vec<UserDto>,
    pub providers: Vec<ProviderDto>,
    pub credentials: Vec<ExportCredentialDto>,
    pub user_keys: Vec<ExportUserKeyDto>,
    pub quotas: Vec<QuotaDto>,
    pub price_rules: Vec<PriceRuleDto>,
    pub price_rates: Vec<PriceRateDto>,
    pub routes: Vec<RouteDto>,
    pub route_members: Vec<RouteMemberDto>,
    pub aliases: Vec<AliasDto>,
    pub model_aliases: Vec<ModelAliasDto>,
    pub routing_rules: Vec<RoutingRuleDto>,
    pub rule_sets: Vec<RuleSetDto>,
    pub rules: Vec<RuleDto>,
    pub provider_rule_sets: Vec<ProviderRuleSetDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ConfigurationExportDto {
    pub format_version: u32,
    pub secrets: SecretExportDto,
    pub source_key: Option<ExportSourceKeyDto>,
    pub data: ConfigurationDataDto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigurationExportRequest {
    pub include_secrets: bool,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ConfigurationImportRequest {
    pub export: ConfigurationExportDto,
    pub source_master_key: Option<String>,
}

impl std::fmt::Debug for ConfigurationImportRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConfigurationImportRequest")
            .field("export", &"<secret-bearing configuration>")
            .field("source_master_key", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigurationImportResponse {
    pub imported: u64,
    pub skipped_credentials: u64,
    pub skipped_user_keys: u64,
}

impl From<gproxy_store::records::CredentialEnvelope> for CredentialEnvelopeDto {
    fn from(value: gproxy_store::records::CredentialEnvelope) -> Self {
        Self {
            ciphertext: value.ciphertext,
            wrapped_key: value.wrapped_key,
            payload_nonce: value.payload_nonce,
            key_nonce: value.key_nonce,
        }
    }
}

impl From<CredentialEnvelopeDto> for gproxy_store::records::CredentialEnvelope {
    fn from(value: CredentialEnvelopeDto) -> Self {
        Self {
            ciphertext: value.ciphertext,
            wrapped_key: value.wrapped_key,
            payload_nonce: value.payload_nonce,
            key_nonce: value.key_nonce,
        }
    }
}
