use serde::Deserialize;
use serde_json::Value;

use super::{default_json_object, default_true};
use crate::store::persistence::records::{
    ProviderRuleSetInput, RoutingRuleInput, RuleInput, RuleSetInput,
};

#[derive(Deserialize)]
pub(crate) struct LegacyRoutingRule {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub operation: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub implementation: String,
    #[serde(default)]
    pub dest_operation: Option<String>,
    #[serde(default)]
    pub dest_kind: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyRoutingRule> for RoutingRuleInput {
    fn from(x: LegacyRoutingRule) -> Self {
        Self {
            id: Some(x.id),
            provider_id: x.provider_id,
            operation: x.operation,
            kind: x.kind,
            implementation: x.implementation,
            dest_operation: x.dest_operation,
            dest_kind: x.dest_kind,
            sort_order: x.sort_order,
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyRuleSet {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

impl From<LegacyRuleSet> for RuleSetInput {
    fn from(x: LegacyRuleSet) -> Self {
        Self {
            id: Some(x.id),
            name: x.name,
            enabled: x.enabled,
            description: x.description,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyRule {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub rule_set_id: i64,
    #[serde(default)]
    pub kind: String,
    #[serde(default = "default_json_object")]
    pub config_json: Value,
    #[serde(default)]
    pub filter_model_pattern: Option<String>,
    #[serde(default)]
    pub filter_operation_keys: Option<Value>,
    #[serde(default)]
    pub filter_header_pattern: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyRule> for RuleInput {
    fn from(x: LegacyRule) -> Self {
        Self {
            id: Some(x.id),
            rule_set_id: x.rule_set_id,
            kind: x.kind,
            config_json: x.config_json,
            filter_model_pattern: x.filter_model_pattern,
            filter_operation_keys: x.filter_operation_keys,
            filter_header_pattern: x.filter_header_pattern,
            sort_order: x.sort_order,
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyProviderRuleSet {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub rule_set_id: i64,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyProviderRuleSet> for ProviderRuleSetInput {
    fn from(x: LegacyProviderRuleSet) -> Self {
        Self {
            id: Some(x.id),
            provider_id: x.provider_id,
            rule_set_id: x.rule_set_id,
            sort_order: x.sort_order,
            enabled: x.enabled,
        }
    }
}
