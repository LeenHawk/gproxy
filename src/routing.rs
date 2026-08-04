//! Host-owned provider routing policy.
//!
//! Wire transforms intentionally do not know about stored database rules,
//! provider ids, ordering, or host logging policy.

use serde_json::Value;

pub use crate::channel_api::routes::RoutingDecision;
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

/// Storage-agnostic routing-rule row consumed by the host.
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
/// warning so one bad stored rule cannot invalidate the host snapshot.
pub fn compile(rows: &[RoutingRuleSpec<'_>]) -> Vec<CompiledRoutingRule> {
    let mut rows: Vec<&RoutingRuleSpec<'_>> = rows.iter().filter(|row| row.enabled).collect();
    rows.sort_by_key(|row| row.sort_order);
    rows.into_iter()
        .filter_map(|row| {
            compile_row(row).or_else(|| {
                tracing::warn!(
                    rule_id = row.id,
                    provider_id = row.provider_id,
                    "skipping unparsable routing rule"
                );
                None
            })
        })
        .collect()
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
            Some(value) => Some(parse_str(value)?),
            None => None,
        },
        dest_kind: match row.dest_kind {
            Some(value) => Some(parse_str(value)?),
            None => None,
        },
    })
}

fn parse_str<T: serde::de::DeserializeOwned>(value: &str) -> Option<T> {
    serde_json::from_value(Value::String(value.to_owned())).ok()
}

/// Decide how the host should service `source` using its stored rules.
pub fn decide(rules: &[CompiledRoutingRule], source: OperationKey) -> RoutingDecision {
    let Some(rule) = rules
        .iter()
        .find(|rule| rule.operation == source.operation() && rule.kind == source.kind())
    else {
        return RoutingDecision::Unsupported;
    };

    match rule.implementation {
        RuleImpl::Passthrough => RoutingDecision::Passthrough,
        RuleImpl::Local => RoutingDecision::Local,
        RuleImpl::Unsupported => RoutingDecision::Unsupported,
        RuleImpl::TransformTo => {
            let Some(kind) = rule.dest_kind else {
                return RoutingDecision::Unsupported;
            };
            let operation = rule.dest_operation.unwrap_or(source.operation());
            OperationKey::try_new(operation, kind)
                .map(RoutingDecision::TransformTo)
                .unwrap_or(RoutingDecision::Unsupported)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ContentGenerationKind;

    fn cg(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
        OperationKey::content_generation(operation, kind)
    }

    #[test]
    fn no_rule_is_unsupported() {
        let source = cg(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        assert_eq!(decide(&[], source), RoutingDecision::Unsupported);
    }

    #[test]
    fn inconsistent_transform_destination_is_unsupported() {
        let source = cg(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        );
        let rule = CompiledRoutingRule {
            operation: source.operation(),
            kind: source.kind(),
            implementation: RuleImpl::TransformTo,
            dest_operation: Some(Operation::CreateEmbedding),
            dest_kind: Some(OperationKind::ContentGeneration(
                ContentGenerationKind::OpenAiResponses,
            )),
        };
        assert_eq!(decide(&[rule], source), RoutingDecision::Unsupported);
    }
}
