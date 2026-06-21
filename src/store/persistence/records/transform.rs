//! Transform records (§8-B2): the keep-as-is `routing_rules` transform-dispatch
//! decision, plus the reusable rule-set model (`rule_sets` → `rules`) attached
//! to providers via `provider_rule_sets`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A per-provider routing rule mapping an `(operation, kind)` to an
/// implementation, optionally rewriting the destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: i64,
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: String,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Upsert input for a routing rule (unique per `(provider_id, operation, kind)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRuleInput {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub operation: String,
    pub kind: String,
    pub implementation: String,
    pub dest_operation: Option<String>,
    pub dest_kind: Option<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

/// A reusable, named set of mutation rules, attachable to many providers via
/// `provider_rule_sets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSet {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a rule set (unique `name`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleSetInput {
    pub id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
}

/// One mutation rule within a [`RuleSet`]. `config_json` carries the
/// kind-specific fields (validated at the process layer, not here):
/// `rewrite`={path,action,value_json?}, `sanitize`={pattern,replacement},
/// `cache_breakpoint`={target,position,index,ttl}, `header`={name,value,mode?},
/// `system_text`={text,position?}.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub id: i64,
    pub rule_set_id: i64,
    pub kind: String,
    pub config_json: Value,
    pub filter_model_pattern: Option<String>,
    #[serde(default)]
    pub filter_operation_keys: Option<Value>,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleInput {
    pub id: Option<i64>,
    pub rule_set_id: i64,
    pub kind: String,
    pub config_json: Value,
    pub filter_model_pattern: Option<String>,
    #[serde(default)]
    pub filter_operation_keys: Option<Value>,
    pub sort_order: i64,
    pub enabled: bool,
}

/// An M:N attachment of a [`RuleSet`] to a provider, applied in `sort_order`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRuleSet {
    pub id: i64,
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a provider ↔ rule-set attachment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderRuleSetInput {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub rule_set_id: i64,
    pub sort_order: i64,
    pub enabled: bool,
}
