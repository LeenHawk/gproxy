mod binding;
mod cycle;
mod health;
mod log;
mod quota;
mod usage;

pub use binding::*;
pub use cycle::*;
pub use health::*;
pub use log::*;
pub use quota::*;
pub use usage::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    AliasRecord, CredentialMetaRecord, ExposedModelRecord, OrganizationRecord, PermissionRecord,
    PriceRateRecord, PriceRuleRecord, ProviderModelRecord, ProviderRecord, ProviderRuleSetRecord,
    QuotaRecord, RateLimitRecord, RouteMemberRecord, RouteRecord, RoutingRuleRecord, RuleRecord,
    RuleSetRecord, TeamRecord, UserKeyRecord, UserRecord,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlSnapshot {
    pub organizations: Vec<OrganizationRecord>,
    pub teams: Vec<TeamRecord>,
    pub providers: Vec<ProviderRecord>,
    pub credentials: Vec<CredentialMetaRecord>,
    pub routes: Vec<RouteRecord>,
    pub route_members: Vec<RouteMemberRecord>,
    pub aliases: Vec<AliasRecord>,
    pub exposed_models: Vec<ExposedModelRecord>,
    pub provider_models: Vec<ProviderModelRecord>,
    pub users: Vec<UserRecord>,
    pub user_keys: Vec<UserKeyRecord>,
    pub permissions: Vec<PermissionRecord>,
    pub rate_limits: Vec<RateLimitRecord>,
    pub quotas: Vec<QuotaRecord>,
    pub price_rules: Vec<PriceRuleRecord>,
    pub price_rates: Vec<PriceRateRecord>,
    pub settings: Vec<SettingRecord>,
    pub routing_rules: Vec<RoutingRuleRecord>,
    pub rule_sets: Vec<RuleSetRecord>,
    pub rules: Vec<RuleRecord>,
    pub provider_rule_sets: Vec<ProviderRuleSetRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingInput {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingRecord {
    pub key: String,
    pub value: Value,
}
