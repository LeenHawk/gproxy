//! v3's operator-made routing rules as v4 operation rules.
//!
//! v3 kept one row per `(provider, operation, incoming kind)` saying how that
//! request was served: passed through, transformed into another operation and
//! kind, answered locally, or refused. Most rows were **seeded** from the
//! channel's own declarations when the provider was created
//! (`origin = channel_default`), and those stay behind: v4 asks the channel
//! at assembly, and a frozen copy would pin the provider to what v3's channel
//! supported that day. Rows an operator wrote (`origin = operator`) are a
//! decision, and v4 keeps the same decision as an operation rule with the
//! `routing` action: one per `(provider, operation)`, mapping each incoming
//! dialect to a route.
//!
//! Only the database route knows a row's origin; v3's export flattened it,
//! so an exported row is reported rather than guessed at. Each mapping is
//! checked with core's own validator, because import does not check them and
//! an invalid one would only fail when a request reached it.

use std::collections::BTreeMap;

use gproxy_core::convert::{Route, RoutingMappings, validate_mapping};
use gproxy_sdk::dto::OperationRuleDto;
use gproxy_sdk::{Dialect, Operation, OperationKey};

use super::{Report, document, ids};

/// v3's operation kind as a v4 dialect.
fn dialect(kind: &str) -> Option<Dialect> {
    Dialect::from_id(match kind.trim() {
        "openai_responses" => "openai",
        "claude_messages" => "claude",
        "gemini_generate_content" => "gemini",
        other => other,
    })
}

pub fn translate(
    data: &document::Data,
    kept_provider: &dyn Fn(i64) -> bool,
    report: &mut Report,
) -> Vec<OperationRuleDto> {
    let (operator, rest): (Vec<&document::RoutingRule>, Vec<&document::RoutingRule>) = data
        .routing_rules
        .iter()
        .partition(|row| row.origin.as_deref() == Some("operator"));
    let seeded = rest
        .iter()
        .filter(|row| row.origin.as_deref() == Some("channel_default"))
        .count();
    let unknown = rest.len() - seeded;
    // Reported once rather than once per row: a provider has one of these per
    // operation and dialect, and a hundred identical lines would bury the rest.
    if seeded > 0 {
        report.drop_row(
            "routing_rules",
            format!("{seeded} rows seeded from channel defaults"),
            "v4 asks the channel at assembly and only stores a row to override it; \
             importing v3's copy would pin every provider to what v3 supported",
        );
    }
    if unknown > 0 {
        report.drop_row(
            "routing_rules",
            format!("{unknown} rows of unknown origin"),
            "v3's export does not say whether a rule was an operator's or a seeded \
             channel default; migrate from the v3 database file to carry operator rules",
        );
    }

    let mut ordered = operator;
    ordered.sort_by_key(|row| (row.sort_order, row.id));
    let mut grouped: BTreeMap<(i64, Operation), (i64, RoutingMappings)> = BTreeMap::new();
    for row in ordered {
        let named = format!(
            "routing rule {} (provider {}, {} {})",
            row.id, row.provider_id, row.operation, row.kind
        );
        if !row.enabled {
            report.drop_row("routing_rules", named, "it was disabled");
            continue;
        }
        if !kept_provider(row.provider_id) {
            report.drop_row("routing_rules", named, "its provider was left behind");
            continue;
        }
        let (Some(operation), Some(source)) =
            (Operation::from_id(row.operation.trim()), dialect(&row.kind))
        else {
            report.drop_row(
                "routing_rules",
                named,
                "v4 has no such operation or dialect",
            );
            continue;
        };
        let route = match row.implementation.trim() {
            "passthrough" => Route::Passthrough,
            "local" => Route::Local,
            "unsupported" => Route::Unsupported,
            "transform_to" | "transform" => {
                let target = row
                    .dest_operation
                    .as_deref()
                    .and_then(|op| Operation::from_id(op.trim()))
                    .zip(row.dest_kind.as_deref().and_then(dialect));
                let Some((operation, dialect)) = target else {
                    report.drop_row(
                        "routing_rules",
                        named,
                        "its destination is not an operation and dialect v4 has",
                    );
                    continue;
                };
                Route::TransformTo {
                    target: OperationKey { operation, dialect },
                }
            }
            other => {
                report.drop_row(
                    "routing_rules",
                    named,
                    format!("unknown implementation `{other}`"),
                );
                continue;
            }
        };
        if let Err(reason) = validate_mapping(
            OperationKey {
                operation,
                dialect: source,
            },
            route,
        ) {
            report.drop_row("routing_rules", named, reason);
            continue;
        }
        let (_, mappings) = grouped
            .entry((row.provider_id, operation))
            .or_insert_with(|| (row.id, RoutingMappings::new()));
        // v3 took the first row for a key in sort order.
        mappings.entry(source).or_insert(route);
    }

    grouped
        .into_iter()
        .map(
            |((provider_id, operation), (first_id, mappings))| OperationRuleDto {
                id: ids::id("routing_rules", first_id),
                provider_id: ids::id("providers", provider_id),
                operation: operation.id().to_owned(),
                action: "routing".into(),
                target: Some(serde_json::to_value(mappings).expect("routes serialize")),
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn operator_rules_become_one_routing_rule_per_operation() {
        let data: document::Data = serde_json::from_value(json!({"routing_rules": [
            {"id": 1, "provider_id": 1, "operation": "generate_content", "kind": "claude_messages",
             "implementation": "transform_to", "dest_operation": "generate_content",
             "dest_kind": "openai_chat", "enabled": true, "origin": "operator"},
            {"id": 2, "provider_id": 1, "operation": "generate_content", "kind": "gemini_generate_content",
             "implementation": "unsupported", "enabled": true, "origin": "operator"},
            {"id": 3, "provider_id": 1, "operation": "generate_content", "kind": "openai_chat",
             "implementation": "passthrough", "enabled": true, "origin": "channel_default"},
            {"id": 4, "provider_id": 1, "operation": "list_models", "kind": "openai",
             "implementation": "passthrough", "enabled": true}
        ]}))
        .unwrap();
        let mut report = Report::default();
        let rules = translate(&data, &|_| true, &mut report);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].operation, "generate_content");
        assert_eq!(rules[0].action, "routing");
        assert_eq!(
            rules[0].target,
            Some(json!({
                "claude": {"implementation": "transform_to",
                           "target": {"operation": "generate_content", "dialect": "openai_chat"}},
                "gemini": {"implementation": "unsupported"}
            }))
        );
        assert_eq!(
            report.dropped.len(),
            2,
            "the seeded row and the one of unknown origin"
        );
    }
}
