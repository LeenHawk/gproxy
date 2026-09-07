use gproxy_store::records::*;
use rust_decimal::Decimal;
use serde_json::Value;
use zeroize::Zeroize;

pub(super) struct Legacy<T> {
    pub id: i64,
    pub value: T,
}

pub(super) struct Credential {
    pub provider_id: i64,
    pub label: Option<String>,
    pub kind: String,
    pub stored_secret: Value,
    pub weight: i64,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

pub(super) struct UserKey {
    pub user_id: i64,
    pub stored_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

impl Drop for UserKey {
    fn drop(&mut self) {
        self.stored_key.zeroize();
    }
}

pub(super) struct Alias {
    pub provider: String,
    pub alias: String,
    pub target: String,
    pub sort_order: i64,
    pub enabled: bool,
}

pub(super) struct PriceRule {
    pub provider_id: Option<i64>,
    pub match_type: String,
    pub model_match: String,
    pub tiers: Option<Value>,
    pub legacy_prices: [Decimal; 7],
    pub enabled: bool,
}

pub(super) struct PriceRate {
    pub rule_id: i64,
    pub metric: String,
    pub unit_size: i64,
    pub price: Decimal,
    pub conditions: Option<Value>,
    pub sort_order: i64,
}

pub(super) struct Quota {
    pub scope: String,
    pub scope_id: i64,
    pub quota_total: Decimal,
    pub quota_daily: Option<Decimal>,
    pub quota_weekly: Option<Decimal>,
    pub quota_monthly: Option<Decimal>,
    pub quota_5h: Option<Decimal>,
    pub quota_7d: Option<Decimal>,
}

pub(super) struct Permission {
    pub scope: String,
    pub scope_id: i64,
    pub route_pattern: String,
}

pub(super) struct Usage {
    pub request_id: String,
    pub at: i64,
    pub route_name: Option<String>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub thread_id: Option<String>,
    pub operation: String,
    pub kind: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub image_output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_30m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    pub metrics: Value,
    pub cost: Decimal,
    pub latency_ms: i64,
    pub usage_source: String,
    pub ended: String,
}

pub(super) struct ProviderModel {
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub variants: Option<Value>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub thinking_supported: Option<bool>,
    pub thinking_adaptive_supported: Option<bool>,
    pub thinking_enabled_supported: Option<bool>,
    pub enabled: bool,
}

pub(super) struct Settings {
    pub instance_name: String,
    pub proxy: Option<String>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub disable_log_redaction: bool,
    pub enable_tokenizer_download: bool,
    pub update_channel: Option<String>,
    pub enable_auto_update_check: bool,
    pub retention_days: Option<i64>,
    pub max_database_size_mb: Option<i64>,
    pub file_upload_max_in_flight: i64,
}

#[derive(Default)]
pub(super) struct SourceData {
    pub downstream_requests: usize,
    pub upstream_requests: usize,
    pub log_references: std::collections::BTreeSet<(Option<i64>, Option<i64>)>,
    pub skipped: Vec<super::report::SkippedTable>,
    pub table_issues: Vec<super::report::ImportIssue>,
    pub organizations: Vec<Legacy<OrganizationInput>>,
    pub teams: Vec<Legacy<TeamInput>>,
    pub users: Vec<Legacy<UserInput>>,
    pub user_keys: Vec<Legacy<UserKey>>,
    pub providers: Vec<Legacy<ProviderInput>>,
    pub credentials: Vec<Legacy<Credential>>,
    pub routes: Vec<Legacy<RouteInput>>,
    pub route_members: Vec<Legacy<RouteMemberInput>>,
    pub aliases: Vec<Legacy<Alias>>,
    pub provider_models: Vec<Legacy<ProviderModel>>,
    pub quotas: Vec<Legacy<Quota>>,
    pub permissions: Vec<Legacy<Permission>>,
    pub price_rules: Vec<Legacy<PriceRule>>,
    pub price_rates: Vec<Legacy<PriceRate>>,
    pub routing_rules: Vec<Legacy<RoutingRuleInput>>,
    pub rule_sets: Vec<Legacy<RuleSetInput>>,
    pub rules: Vec<Legacy<RuleInput>>,
    pub provider_rule_sets: Vec<Legacy<ProviderRuleSetInput>>,
    pub settings: Vec<Legacy<Settings>>,
    pub usage: Vec<Legacy<Usage>>,
    pub usage_tombstone_providers: Vec<Legacy<ProviderInput>>,
    pub usage_tombstone_credentials: Vec<Legacy<Credential>>,
}
