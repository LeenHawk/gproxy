//! Payload rewrite rule sets, their rules and the provider attachments.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::upstream::{provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RuleSetDto {
    pub id: String,
    pub name: String,
    /// Number of provider attachments, populated by list/get.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub provider_count: Option<u64>,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl From<rewrite_rule_set::Model> for RuleSetDto {
    fn from(row: rewrite_rule_set::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            provider_count: None,
            description: row.description,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RuleSetWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RuleSetPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// One ordered replacement. Every write compiles the rule with core's own
/// compiler first, so a rule that would be skipped at assembly is refused
/// here instead of silently doing nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RewriteRuleDto {
    pub id: String,
    pub rule_set_id: String,
    /// `request`, `response` or `both`.
    pub phase: String,
    #[serde(default = "default_action")]
    pub action: String,
    /// `body`, `header` or `query`.
    pub target: String,
    pub target_name: Option<String>,
    /// JSON array of dot paths; body target only.
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub paths: Option<Value>,
    pub pattern: String,
    pub replacement: String,
    /// JSON array of `{"operation": …, "dialect": …}`.
    #[cfg_attr(
        feature = "ts",
        ts(type = "{ operation: string, dialect: string }[] | null")
    )]
    pub filter_operation_keys: Option<Value>,
    pub filter_model_pattern: Option<String>,
    pub filter_header_pattern: Option<String>,
    pub filter_event_pattern: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ path: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: unknown } | null"
        )
    )]
    pub filter_body: Option<Value>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ name: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: string } | null"
        )
    )]
    pub filter_header: Option<Value>,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl From<rewrite_rule::Model> for RewriteRuleDto {
    fn from(row: rewrite_rule::Model) -> Self {
        Self {
            id: row.id,
            rule_set_id: row.rule_set_id,
            phase: row.phase,
            action: row.action,
            target: target_name(row.target).to_owned(),
            target_name: row.target_name,
            paths: row.paths,
            pattern: row.pattern,
            replacement: row.replacement,
            filter_operation_keys: row.filter_operation_keys,
            filter_model_pattern: row.filter_model_pattern,
            filter_header_pattern: row.filter_header_pattern,
            filter_event_pattern: row.filter_event_pattern,
            filter_body: row.filter_body,
            filter_header: row.filter_header,
            sort_order: row.sort_order,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
        }
    }
}

pub(crate) fn target_name(target: rewrite_rule::RewriteTarget) -> &'static str {
    match target {
        rewrite_rule::RewriteTarget::Body => "body",
        rewrite_rule::RewriteTarget::Header => "header",
        rewrite_rule::RewriteTarget::Query => "query",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RewriteRuleWrite {
    #[serde(default)]
    pub id: Option<String>,
    /// Ignored by `replace_rules`, which owns the set it is replacing.
    #[serde(default)]
    pub rule_set_id: Option<String>,
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub target_name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub paths: Option<Value>,
    pub pattern: String,
    pub replacement: String,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(type = "{ operation: string, dialect: string }[] | null")
    )]
    pub filter_operation_keys: Option<Value>,
    #[serde(default)]
    pub filter_model_pattern: Option<String>,
    #[serde(default)]
    pub filter_header_pattern: Option<String>,
    #[serde(default)]
    pub filter_event_pattern: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ path: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: unknown } | null"
        )
    )]
    pub filter_body: Option<Value>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ name: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: string } | null"
        )
    )]
    pub filter_header: Option<Value>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RewriteRulePatch {
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub target_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(feature = "ts", ts(type = "string[] | null"))]
    pub paths: Option<Option<Value>>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub replacement: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(type = "{ operation: string, dialect: string }[] | null")
    )]
    pub filter_operation_keys: Option<Option<Value>>,
    #[serde(default, deserialize_with = "double_option")]
    pub filter_model_pattern: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub filter_header_pattern: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub filter_event_pattern: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ path: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: unknown } | null"
        )
    )]
    pub filter_body: Option<Option<Value>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ name: string; op: \"eq\" | \"ne\" | \"exists\" | \"not_exists\"; value?: string } | null"
        )
    )]
    pub filter_header: Option<Option<Value>>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A rule set attached to a provider, in attachment order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderRuleSetDto {
    pub id: String,
    pub provider_id: String,
    pub rule_set_id: String,
    pub sort_order: i64,
    pub enabled: bool,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

impl From<provider_rewrite_rule_set::Model> for ProviderRuleSetDto {
    fn from(row: provider_rewrite_rule_set::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            rule_set_id: row.rule_set_id,
            sort_order: row.sort_order,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
            updated_at_ms: row.updated_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderRuleSetWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub rule_set_id: String,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderRuleSetPatch {
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

fn default_action() -> String {
    "replace".into()
}
