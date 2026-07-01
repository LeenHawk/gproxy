//! Compiled routing rules (§8-B2 `routing_rules`) and the transform-dispatch
//! decision (§6.1): passthrough / transform_to / local / unsupported.

use serde_json::Value;

use crate::protocol::{Operation, OperationKey, OperationKind};

/// `routing_rules.implementation`, parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleImpl {
    Passthrough,
    TransformTo,
    Local,
    Unsupported,
}

/// A routing-rule row with its string fields parsed into protocol enums.
#[derive(Debug, Clone)]
pub struct CompiledRoutingRule {
    pub operation: Operation,
    pub kind: OperationKind,
    pub implementation: RuleImpl,
    pub dest_operation: Option<Operation>,
    pub dest_kind: Option<OperationKind>,
}

/// Storage-agnostic routing-rule row consumed by the transform crate.
pub struct RoutingRuleSpec<'a> {
    pub id: i64,
    pub provider_id: i64,
    pub operation: &'a str,
    pub kind: &'a str,
    pub implementation: &'a str,
    pub dest_operation: Option<&'a str>,
    pub dest_kind: Option<&'a str>,
    pub sort_order: i64,
    pub enabled: bool,
}

/// Parse enabled rows in `sort_order`. Unparsable rows are skipped with a
/// warning — bad config must not take the snapshot down.
pub fn compile(rows: &[RoutingRuleSpec<'_>]) -> Vec<CompiledRoutingRule> {
    let mut rows: Vec<&RoutingRuleSpec<'_>> = rows.iter().filter(|r| r.enabled).collect();
    rows.sort_by_key(|r| r.sort_order);
    let mut out = Vec::new();
    for row in rows {
        match compile_row(row) {
            Some(rule) => out.push(rule),
            None => tracing::warn!(
                rule_id = row.id,
                provider_id = row.provider_id,
                "skipping unparsable routing rule"
            ),
        }
    }
    out
}

fn compile_row(row: &RoutingRuleSpec<'_>) -> Option<CompiledRoutingRule> {
    Some(CompiledRoutingRule {
        operation: parse_str(row.operation)?,
        kind: parse_str(row.kind)?,
        implementation: match row.implementation {
            "passthrough" => RuleImpl::Passthrough,
            "transform_to" => RuleImpl::TransformTo,
            "local" => RuleImpl::Local,
            "unsupported" => RuleImpl::Unsupported,
            _ => return None,
        },
        dest_operation: match row.dest_operation {
            Some(s) => Some(parse_str(s)?),
            None => None,
        },
        dest_kind: match row.dest_kind {
            Some(s) => Some(parse_str(s)?),
            None => None,
        },
    })
}

/// Protocol enums all serde-rename to snake_case strings (`"claude_messages"`,
/// `"open_ai"`, …) — reuse that as the single parse path. `OperationKind` is
/// `#[serde(untagged)]`, so a plain string tries ContentGenerationKind first,
/// then Provider, exactly matching the §8 kind vocabulary.
fn parse_str<T: serde::de::DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(Value::String(s.to_owned())).ok()
}

/// The dispatch decision for one `(source op, target channel kind)` pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingDecision {
    Passthrough,
    TransformTo(OperationKey),
    Local,
    Unsupported,
}

/// Decide how to service `source` on a channel. Routing is driven entirely by
/// stored rules: an explicit rule wins; **no matching rule means `Unsupported`**.
/// Channel defaults are materialized into real rules at provider creation (see
/// [`crate::api::routing::seed_default_routing`]) — they are not recomputed here.
/// A `transform_to` rule whose `dest_kind` is missing is malformed and yields
/// `Unsupported`.
pub fn decide(rules: &[CompiledRoutingRule], source: OperationKey) -> RoutingDecision {
    if let Some(rule) = rules
        .iter()
        .find(|r| r.operation == source.operation && r.kind == source.kind)
    {
        return match rule.implementation {
            RuleImpl::Passthrough => RoutingDecision::Passthrough,
            RuleImpl::Local => RoutingDecision::Local,
            RuleImpl::Unsupported => RoutingDecision::Unsupported,
            RuleImpl::TransformTo => match rule.dest_kind {
                Some(kind) => RoutingDecision::TransformTo(OperationKey {
                    operation: rule.dest_operation.unwrap_or(source.operation),
                    kind,
                }),
                None => RoutingDecision::Unsupported,
            },
        };
    }
    RoutingDecision::Unsupported
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ContentGenerationKind;

    fn cg(op: Operation, k: ContentGenerationKind) -> OperationKey {
        OperationKey::content_generation(op, k)
    }

    #[test]
    fn no_rule_is_unsupported() {
        use ContentGenerationKind as K;
        // At request time, an unseeded cell (no matching rule) is unsupported.
        let src = cg(Operation::GenerateContent, K::ClaudeMessages);
        assert_eq!(decide(&[], src), RoutingDecision::Unsupported);
    }

    #[test]
    fn explicit_rule_wins() {
        let rule = CompiledRoutingRule {
            operation: Operation::GenerateContent,
            kind: OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages),
            implementation: RuleImpl::Unsupported,
            dest_operation: None,
            dest_kind: None,
        };
        let src = cg(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        assert_eq!(decide(&[rule], src), RoutingDecision::Unsupported);
    }
}
