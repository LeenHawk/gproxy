use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRuleInput {
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: String,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRuleRecord {
    pub id: i64,
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: String,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
    pub origin: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetInput {
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSetRecord {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleInput {
    pub rule_set_id: i64,
    pub kind: String,
    pub config: Value,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleRecord {
    pub id: i64,
    pub rule_set_id: i64,
    pub kind: String,
    pub config: Value,
    pub filter_model_pattern: Option<String>,
    pub filter_operations: Option<Vec<String>>,
    pub filter_header_pattern: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuleSetInput {
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuleSetRecord {
    pub id: i64,
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
    pub origin: String,
    pub created_at: i64,
    pub updated_at: i64,
}
