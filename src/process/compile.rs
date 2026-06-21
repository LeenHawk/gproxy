//! Rule-set compilation: parse `rules.config_json` into typed configs at
//! snapshot-build time so the hot path never re-parses or re-compiles regexes.

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

use crate::protocol::{Operation, OperationKey};
use crate::store::persistence::records::Rule;

/// `cache_breakpoint` config (claude-only semantics).
#[derive(Debug, Clone, Deserialize)]
pub struct CacheBreakpointCfg {
    /// "system" | "tools" | "last_message"
    pub target: String,
    /// Block index within the target; default = last block.
    #[serde(default)]
    pub index: Option<i64>,
    /// e.g. "5m" | "1h"
    #[serde(default)]
    pub ttl: Option<String>,
    /// Reserved (v1 compat); unused in M2.
    #[serde(default)]
    pub position: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RewriteAction {
    Set,
    Delete,
    Merge,
}

/// Where to insert text relative to existing content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextPosition {
    #[default]
    Prepend,
    Append,
}

/// How to apply a header value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderMode {
    /// Insert or replace the header value.
    #[default]
    Override,
    /// Comma-join with dedup (for list-valued headers like `anthropic-beta`).
    Merge,
}

/// One parsed rule body.
#[derive(Debug, Clone)]
pub enum RuleConfig {
    SystemText {
        text: String,
        position: TextPosition,
    },
    CacheBreakpoint(CacheBreakpointCfg),
    Rewrite {
        path: String,
        action: RewriteAction,
        value_json: Option<Value>,
    },
    Sanitize {
        regex: Regex,
        replacement: String,
    },
    Header {
        name: http::header::HeaderName,
        value: String,
        mode: HeaderMode,
    },
}

impl RuleConfig {
    /// Fixed application order (§6.1).
    pub fn rank(&self) -> u8 {
        match self {
            Self::SystemText { .. } => 0,
            Self::CacheBreakpoint(_) => 1,
            Self::Rewrite { .. } => 2,
            Self::Sanitize { .. } => 3,
            Self::Header { .. } => 4,
        }
    }

    /// Ranks 0–2 mutate the parsed JSON body.
    pub fn mutates_value(&self) -> bool {
        self.rank() <= 2
    }
}

/// A rule ready for the hot path.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub config: RuleConfig,
    model_pattern: Option<String>,
    operations: Option<Vec<Operation>>,
}

impl CompiledRule {
    /// `filter_operation_keys` matches the TARGET operation;
    /// `filter_model_pattern` glob-matches the (prefix-stripped) upstream model.
    pub fn matches(&self, op: OperationKey, model: &str) -> bool {
        if let Some(ops) = &self.operations
            && !ops.contains(&op.operation)
        {
            return false;
        }
        if let Some(p) = &self.model_pattern
            && !crate::util::glob::matches(p, model)
        {
            return false;
        }
        true
    }
}

/// Compile one rule set's rows: enabled only, in `sort_order`. Unparsable
/// rules are skipped with a warning.
pub fn compile_rules(rows: &[Rule]) -> Vec<CompiledRule> {
    let mut rows: Vec<&Rule> = rows.iter().filter(|r| r.enabled).collect();
    rows.sort_by_key(|r| r.sort_order);
    let mut out = Vec::new();
    for row in rows {
        match compile_row(row) {
            Some(rule) => out.push(rule),
            None => tracing::warn!(
                rule_id = row.id,
                kind = %row.kind,
                "skipping unparsable process rule"
            ),
        }
    }
    out
}

/// Stable-sort a provider's flattened rules into fixed kind order, preserving
/// (set sort_order, rule sort_order) within each kind. Call after flattening
/// the provider's attached sets (snapshot build).
pub fn order_for_apply(rules: &mut [CompiledRule]) {
    rules.sort_by_key(|r| r.config.rank());
}

fn compile_row(row: &Rule) -> Option<CompiledRule> {
    let config = match row.kind.as_str() {
        "system_text" => {
            #[derive(Deserialize)]
            struct Raw {
                text: String,
                #[serde(default)]
                position: TextPosition,
            }
            let raw: Raw = serde_json::from_value(row.config_json.clone()).ok()?;
            RuleConfig::SystemText {
                text: raw.text,
                position: raw.position,
            }
        }
        "cache_breakpoint" => {
            RuleConfig::CacheBreakpoint(serde_json::from_value(row.config_json.clone()).ok()?)
        }
        "rewrite" => {
            #[derive(Deserialize)]
            struct Raw {
                path: String,
                action: RewriteAction,
                #[serde(default)]
                value_json: Option<Value>,
            }
            let raw: Raw = serde_json::from_value(row.config_json.clone()).ok()?;
            RuleConfig::Rewrite {
                path: raw.path,
                action: raw.action,
                value_json: raw.value_json,
            }
        }
        "sanitize" => {
            #[derive(Deserialize)]
            struct Raw {
                pattern: String,
                replacement: String,
            }
            let raw: Raw = serde_json::from_value(row.config_json.clone()).ok()?;
            RuleConfig::Sanitize {
                regex: Regex::new(&raw.pattern).ok()?,
                replacement: raw.replacement,
            }
        }
        "header" => {
            #[derive(Deserialize)]
            struct Raw {
                name: String,
                value: String,
                #[serde(default)]
                mode: HeaderMode,
            }
            let raw: Raw = serde_json::from_value(row.config_json.clone()).ok()?;
            let name = http::header::HeaderName::from_bytes(raw.name.as_bytes()).ok()?;
            RuleConfig::Header {
                name,
                value: raw.value,
                mode: raw.mode,
            }
        }
        _ => return None,
    };
    let operations = match &row.filter_operation_keys {
        None | Some(Value::Null) => None,
        Some(v) => Some(serde_json::from_value::<Vec<Operation>>(v.clone()).ok()?),
    };
    Some(CompiledRule {
        config,
        model_pattern: row.filter_model_pattern.clone(),
        operations,
    })
}
