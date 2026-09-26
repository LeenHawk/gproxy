//! v3's rewrite rules as v4's.
//!
//! Every v3 rule kind has a v4 action now: `transform` is `replace` (or `set`
//! when it replaced the whole located value), `rewrite` is `set` / `delete` /
//! `merge` over a dot path, `header` is `header_set` / `header_merge`, and
//! `system_text` / `cache_breakpoint` are the content actions of the same
//! names. Paths are dot paths on both sides.
//!
//! Two differences are bridged rather than copied:
//!
//! - **Dialect.** v3 read the dialect off the request when it applied a
//!   `system_text` or `cache_breakpoint` rule; v4 names it in the rule. One v3
//!   rule becomes one v4 rule per dialect v3 would have applied it to, minus
//!   the ones v4 refuses for its settings (a TTL only Claude takes, the
//!   Claude-only `tools` target) — v3 applied those as no-ops there.
//! - **Operation filter.** v3 filtered on the operation alone; v4 filters on
//!   `(operation, dialect)` pairs. Every dialect is listed, which is what the
//!   operation alone meant.
//!
//! Reported instead of translated: a transform `limit` (v4 replaces every
//! match; carrying the rule would change how much it rewrites), a cache
//! `index` of `0` (v3 selected nothing with it), and values v4's compiler
//! would refuse, which would otherwise fail the whole snapshot on import.

use gproxy_sdk::dto::{RewriteRuleDto, RuleSetDto};
use serde_json::{Value, json};

use super::{
    Report,
    document::{self, Action, Locate, RuleConfig},
    ids,
};

/// Every v4 dialect, for an operation filter that did not name one.
const DIALECTS: [&str; 5] = [
    "openai",
    "openai_chat",
    "openai_responses_websocket",
    "claude",
    "gemini",
];
/// The dialects v3 applied system text to.
const SYSTEM_TEXT_DIALECTS: [&str; 5] = DIALECTS;
/// The dialects v3 placed cache breakpoints in; Gemini has none.
const CACHE_DIALECTS: [&str; 4] = [
    "openai",
    "openai_chat",
    "openai_responses_websocket",
    "claude",
];

/// One v4 rule before the columns every rule of a row shares.
struct Draft {
    action: &'static str,
    phase: &'static str,
    target: &'static str,
    target_name: Option<String>,
    paths: Option<Vec<String>>,
    pattern: String,
    replacement: String,
    /// The dialect a content action is bound to, which narrows the
    /// operation filter to that dialect.
    dialect: Option<&'static str>,
}

impl Draft {
    fn body(action: &'static str, phase: &'static str, paths: Option<Vec<String>>) -> Self {
        Self {
            action,
            phase,
            target: "body",
            target_name: None,
            paths,
            pattern: String::new(),
            replacement: String::new(),
            dialect: None,
        }
    }
}

fn phase(v3: &str) -> &'static str {
    match v3 {
        "response" => "response",
        "both" => "both",
        _ => "request",
    }
}

pub fn translate(
    data: &document::Data,
    report: &mut Report,
) -> (Vec<RewriteRuleDto>, Vec<RuleSetDto>) {
    let sets = data
        .rule_sets
        .iter()
        .map(|row| RuleSetDto {
            id: ids::id("rule_sets", row.id),
            name: row.name.clone(),
            description: row.description.clone(),
            enabled: row.enabled,
            created_at_ms: 0,
            updated_at_ms: 0,
        })
        .collect();

    let mut rules = Vec::new();
    for row in &data.rules {
        let named = format!("rule {} ({})", row.id, row.config.kind());
        let operations = match operations(row, &named, report) {
            Ok(operations) => operations,
            Err(()) => continue,
        };
        let drafts = match drafts(&row.config, &named, report) {
            Some(drafts) if !drafts.is_empty() => drafts,
            _ => continue,
        };
        for (index, draft) in drafts.into_iter().enumerate() {
            let filter_operation_keys = operations.as_ref().map(|operations| {
                let dialects: Vec<&str> = match draft.dialect {
                    Some(dialect) => vec![dialect],
                    None => DIALECTS.to_vec(),
                };
                Value::Array(
                    operations
                        .iter()
                        .flat_map(|operation| {
                            dialects.iter().map(
                                move |dialect| json!({"operation": operation, "dialect": dialect}),
                            )
                        })
                        .collect(),
                )
            });
            rules.push(RewriteRuleDto {
                id: ids::part("rules", row.id, &index.to_string()),
                rule_set_id: ids::id("rule_sets", row.rule_set_id),
                action: draft.action.into(),
                phase: draft.phase.into(),
                target: draft.target.into(),
                target_name: draft.target_name,
                paths: draft
                    .paths
                    .map(|paths| Value::Array(paths.into_iter().map(Value::from).collect())),
                pattern: draft.pattern,
                replacement: draft.replacement,
                filter_operation_keys,
                filter_model_pattern: row.filter_model_pattern.clone(),
                filter_header_pattern: row.filter_header_pattern.clone(),
                filter_event_pattern: None,
                sort_order: row.sort_order,
                enabled: row.enabled,
                created_at_ms: 0,
                updated_at_ms: 0,
            });
        }
    }
    (rules, sets)
}

/// v3's operation filter as v4 operation ids. `Err` when the row is dropped.
fn operations(
    row: &document::Rule,
    named: &str,
    report: &mut Report,
) -> Result<Option<Vec<String>>, ()> {
    let Some(v3) = row.filter_operations.as_ref().filter(|ops| !ops.is_empty()) else {
        return Ok(None);
    };
    let (known, unknown): (Vec<&String>, Vec<&String>) = v3
        .iter()
        .partition(|operation| gproxy_sdk::Operation::from_id(operation.trim()).is_some());
    if known.is_empty() {
        report.drop_row(
            "rules",
            named.to_owned(),
            format!(
                "it was filtered to operations v4 does not have ({})",
                v3.join(", ")
            ),
        );
        return Err(());
    }
    if !unknown.is_empty() {
        report.warn(format!(
            "{named}: operations v4 does not have were left out of its filter ({})",
            unknown
                .iter()
                .map(|op| op.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(Some(
        known.into_iter().map(|op| op.trim().to_owned()).collect(),
    ))
}

/// The v4 rules one v3 rule becomes. `None` when it was reported as dropped.
fn drafts(config: &RuleConfig, named: &str, report: &mut Report) -> Option<Vec<Draft>> {
    let mut drop = |reason: String| {
        report.drop_row("rules", named.to_owned(), reason);
        None
    };
    match config {
        RuleConfig::SystemText { text, position } => {
            let position = match position.trim() {
                "" | "prepend" => "prepend",
                "append" => "append",
                other => {
                    return drop(format!(
                        "its position `{other}` is neither prepend nor append"
                    ));
                }
            };
            if text.is_empty() {
                return drop("its text is empty".into());
            }
            Some(
                SYSTEM_TEXT_DIALECTS
                    .iter()
                    .map(|dialect| Draft {
                        replacement:
                            json!({"dialect": dialect, "text": text, "position": position})
                                .to_string(),
                        dialect: Some(dialect),
                        ..Draft::body("system_text", "request", None)
                    })
                    .collect(),
            )
        }
        RuleConfig::CacheBreakpoint { target, index, ttl } => {
            if *index == Some(0) {
                return drop("its index was 0, which v3 resolved to no location".into());
            }
            if !matches!(target.as_str(), "global" | "system" | "message" | "tools") {
                return drop(format!("its target `{target}` is not one v4 places"));
            }
            let drafts: Vec<Draft> = CACHE_DIALECTS
                .iter()
                .filter(|dialect| target != "tools" || **dialect == "claude")
                .filter(|dialect| match ttl.as_deref() {
                    None => true,
                    Some(ttl) if **dialect == "claude" => matches!(ttl, "5m" | "1h"),
                    Some(ttl) => ttl == "30m",
                })
                .map(|dialect| Draft {
                    replacement:
                        json!({"dialect": dialect, "target": target, "index": index, "ttl": ttl})
                            .to_string(),
                    dialect: Some(dialect),
                    ..Draft::body("cache_breakpoint", "request", None)
                })
                .collect();
            if drafts.is_empty() {
                return drop(format!(
                    "no dialect v4 places a breakpoint for accepts its TTL `{}`",
                    ttl.as_deref().unwrap_or_default()
                ));
            }
            Some(drafts)
        }
        RuleConfig::Rewrite {
            path,
            action,
            value,
        } => {
            let paths = Some(vec![path.clone()]);
            let draft = match (action.trim(), value) {
                ("delete", _) => Draft::body("delete", "request", paths),
                ("set", Some(value)) => Draft {
                    replacement: value.to_string(),
                    ..Draft::body("set", "request", paths)
                },
                ("merge", Some(value @ Value::Object(_))) => Draft {
                    replacement: value.to_string(),
                    ..Draft::body("merge", "request", paths)
                },
                (action, _) => {
                    return drop(format!(
                        "`{action}` with this value is not a set, delete or merge v4 can apply"
                    ));
                }
            };
            Some(vec![draft])
        }
        RuleConfig::Header { name, value, mode } => {
            if axum::http::HeaderName::from_bytes(name.trim().as_bytes()).is_err()
                || axum::http::HeaderValue::from_str(value).is_err()
            {
                return drop("its header name or value is not valid in HTTP".into());
            }
            let action = match mode.trim() {
                "" | "override" => "header_set",
                "merge" => "header_merge",
                other => return drop(format!("its mode `{other}` is neither override nor merge")),
            };
            Some(vec![Draft {
                target: "header",
                target_name: Some(name.trim().to_owned()),
                replacement: value.clone(),
                ..Draft::body(action, "request", None)
            }])
        }
        RuleConfig::Transform {
            phase: v3_phase,
            locate,
            actions,
            limit,
        } => {
            if let Some(limit) = limit {
                return drop(format!(
                    "it stopped after {limit} replacements; v4 replaces every match, so \
                     carrying it would rewrite more than it did"
                ));
            }
            let phase = phase(v3_phase);
            let mut drafts = Vec::new();
            for action in actions {
                let draft = match (locate, action) {
                    // v3 applied only literal replacements to a whole-body
                    // match, the match being the thing replaced.
                    (Locate::Match(pattern), Action::ReplaceText { with, .. }) => Draft {
                        pattern: pattern.clone(),
                        replacement: with.clone(),
                        ..Draft::body("replace", phase, None)
                    },
                    (Locate::Match(_), Action::ReplaceRegex { .. }) => continue,
                    (Locate::Path(_) | Locate::Paths(_), action) => {
                        let paths = match locate {
                            Locate::Path(path) => vec![path.clone()],
                            Locate::Paths(paths) => paths.clone(),
                            Locate::Match(_) => unreachable!(),
                        };
                        match action {
                            Action::ReplaceRegex { pattern, with } => Draft {
                                pattern: pattern.clone(),
                                replacement: with.clone(),
                                ..Draft::body("replace", phase, Some(paths))
                            },
                            Action::ReplaceText {
                                from: Some(from),
                                with,
                            } => Draft {
                                pattern: regex::escape(from),
                                replacement: with.clone(),
                                ..Draft::body("replace", phase, Some(paths))
                            },
                            // The whole located value became `with`.
                            Action::ReplaceText { from: None, with } => Draft {
                                replacement: Value::from(with.as_str()).to_string(),
                                ..Draft::body("set", phase, Some(paths))
                            },
                        }
                    }
                };
                drafts.push(draft);
            }
            Some(drafts)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(rules: Value) -> (Vec<RewriteRuleDto>, Report) {
        let data: document::Data = serde_json::from_value(
            json!({"rule_sets": [{"id": 1, "name": "s", "enabled": true}],
                                          "rules": rules}),
        )
        .unwrap();
        let mut report = Report::default();
        let (rules, _) = translate(&data, &mut report);
        (rules, report)
    }

    fn row(config: Value) -> Value {
        json!({"id": 1, "rule_set_id": 1, "config": config, "enabled": true})
    }

    /// Import runs every rule through core's compiler and refuses the whole
    /// document for one it rejects, so every shape this module emits has to
    /// compile.
    fn compiles(rule: &RewriteRuleDto) {
        use gproxy_store::entity::upstream::rewrite_rule::{Model, RewriteTarget};
        let model = Model {
            id: rule.id.clone(),
            rule_set_id: rule.rule_set_id.clone(),
            phase: rule.phase.clone(),
            action: rule.action.clone(),
            target: match rule.target.as_str() {
                "header" => RewriteTarget::Header,
                "query" => RewriteTarget::Query,
                _ => RewriteTarget::Body,
            },
            target_name: rule.target_name.clone(),
            paths: rule.paths.clone(),
            pattern: rule.pattern.clone(),
            replacement: rule.replacement.clone(),
            filter_operation_keys: rule.filter_operation_keys.clone(),
            filter_model_pattern: rule.filter_model_pattern.clone(),
            filter_header_pattern: rule.filter_header_pattern.clone(),
            filter_event_pattern: rule.filter_event_pattern.clone(),
            sort_order: rule.sort_order,
            enabled: rule.enabled,
            created_at_ms: 0,
            updated_at_ms: 0,
        };
        if let Err(error) = gproxy_core::rewrite::compile_rule(std::sync::Arc::new(model)) {
            panic!("{} ({}) does not compile: {error}", rule.id, rule.action);
        }
    }

    #[test]
    fn every_translated_kind_compiles_in_v4() {
        let mut filtered = row(json!({"kind": "system_text", "text": "hi", "position": "prepend"}));
        filtered["filter_operations"] = json!(["generate_content"]);
        let (out, report) = rules(json!([
            filtered,
            row(json!({"kind": "cache_breakpoint", "target": "message", "index": -1})),
            row(json!({"kind": "cache_breakpoint", "target": "system", "ttl": "30m"})),
            row(json!({"kind": "rewrite", "path": "a.b", "action": "set", "value": 1})),
            row(json!({"kind": "rewrite", "path": "a", "action": "delete"})),
            row(json!({"kind": "rewrite", "path": "a", "action": "merge", "value": {"k": true}})),
            row(json!({"kind": "header", "name": "x-a", "value": "1", "mode": "override"})),
            row(
                json!({"kind": "transform", "phase": "both", "locate": {"type": "match", "value": "fo+"},
                       "actions": [{"op": "replace_text", "with": "bar"}]})
            ),
            row(
                json!({"kind": "transform", "phase": "request", "locate": {"type": "paths", "value": ["a", "b.*"]},
                       "actions": [{"op": "replace_text", "with": "x"},
                                   {"op": "replace_regex", "pattern": "y+", "with": "z"}]})
            ),
        ]));
        assert!(report.dropped.is_empty(), "{:?}", report.dropped);
        assert_eq!(out.len(), 5 + 4 + 3 + 3 + 1 + 1 + 2);
        out.iter().for_each(compiles);
    }

    #[test]
    fn content_rules_become_one_rule_per_dialect_v3_applied_them_to() {
        let (out, _) = rules(json!([row(
            json!({"kind": "system_text", "text": "hi", "position": "append"})
        )]));
        assert_eq!(out.len(), 5);
        assert!(
            out.iter()
                .all(|r| r.action == "system_text" && r.paths.is_none())
        );

        let (out, _) = rules(json!([row(
            json!({"kind": "cache_breakpoint", "target": "tools", "ttl": "1h"})
        )]));
        assert_eq!(out.len(), 1, "tools and a 1h TTL are Claude's alone");
        assert!(out[0].replacement.contains("\"claude\""));

        let (out, report) = rules(json!([row(
            json!({"kind": "cache_breakpoint", "target": "message", "index": 0})
        )]));
        assert!(out.is_empty());
        assert_eq!(report.dropped.len(), 1);
    }

    #[test]
    fn structural_header_and_whole_body_rules_carry() {
        let (out, _) = rules(json!([
            row(json!({"kind": "rewrite", "path": "a.b", "action": "set", "value": {"x": 1}})),
            row(json!({"kind": "header", "name": "x-a", "value": "1", "mode": "merge"})),
            row(
                json!({"kind": "transform", "phase": "response", "locate": {"type": "match", "value": "foo"},
                       "actions": [{"op": "replace_text", "with": "bar"}]})
            ),
            row(
                json!({"kind": "transform", "phase": "request", "locate": {"type": "path", "value": "model"},
                       "actions": [{"op": "replace_text", "with": "m2"}]})
            ),
        ]));
        let got: Vec<_> = out
            .iter()
            .map(|r| (r.action.as_str(), r.replacement.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("set", "{\"x\":1}"),
                ("header_merge", "1"),
                ("replace", "bar"),
                ("set", "\"m2\"")
            ]
        );
        assert_eq!(out[1].target_name.as_deref(), Some("x-a"));
        assert!(out[2].paths.is_none() && out[2].pattern == "foo");
    }

    #[test]
    fn an_operation_filter_lists_every_dialect_and_a_limit_is_refused() {
        let mut filtered = row(
            json!({"kind": "transform", "phase": "request", "locate": {"type": "path", "value": "a"},
                                      "actions": [{"op": "replace_regex", "pattern": "x", "with": "y"}]}),
        );
        filtered["filter_operations"] = json!(["generate_content", "remix_video"]);
        let (out, report) = rules(json!([filtered]));
        assert_eq!(
            out[0]
                .filter_operation_keys
                .as_ref()
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            5
        );
        assert_eq!(report.warnings.len(), 1);

        let (out, report) = rules(json!([row(
            json!({"kind": "transform", "phase": "request",
            "locate": {"type": "path", "value": "a"}, "limit": 1,
            "actions": [{"op": "replace_regex", "pattern": "x", "with": "y"}]})
        )]));
        assert!(out.is_empty());
        assert_eq!(report.dropped.len(), 1);
    }
}
