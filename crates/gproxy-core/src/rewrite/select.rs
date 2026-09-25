//! Which rules apply to one exchange, decided before any payload is decoded.

use crate::{CoreData, ProviderData, RewritePhase, RewriteRuleData, RewriteTarget};
use gproxy_protocol::OperationKey;
use http::HeaderMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Request,
    Response,
}

/// Facts known at the entry boundary. `request_headers` are the original
/// inbound headers: a filter condition, never the rewrite target.
pub struct RewriteContext<'a> {
    /// The native operation being executed against the provider.
    pub operation: OperationKey,
    pub upstream_model: Option<&'a str>,
    /// The caller's requested model or alias, matched as an alternative.
    pub requested_model: Option<&'a str>,
    pub request_headers: &'a HeaderMap,
}

/// Rules partitioned by target, each in attachment then rule order.
#[derive(Default)]
pub struct SelectedRules {
    pub body: Vec<Arc<RewriteRuleData>>,
    pub headers: Vec<Arc<RewriteRuleData>>,
    pub query: Vec<Arc<RewriteRuleData>>,
}

impl SelectedRules {
    pub fn is_empty(&self) -> bool {
        self.body.is_empty() && self.headers.is_empty() && self.query.is_empty()
    }
}

/// Walk the provider's enabled attachments in `(sort_order, id)` order and
/// their compiled sets' rules in the same order. Event filters are not
/// evaluated here; they need the decoded unit and stay with `apply_unit`.
pub fn select_rules(
    data: &CoreData,
    provider: &ProviderData,
    phase: Phase,
    context: &RewriteContext<'_>,
) -> SelectedRules {
    let mut selected = SelectedRules::default();
    for attachment in &provider.rewrite_rule_sets {
        let Some(set) = data.rewrite_rule_sets.get(&attachment.rule_set_id) else {
            continue;
        };
        for rule in &set.rules {
            if !applies(rule, phase, context) {
                continue;
            }
            match rule.target {
                RewriteTarget::Body { .. } => selected.body.push(rule.clone()),
                RewriteTarget::Header { .. } => selected.headers.push(rule.clone()),
                RewriteTarget::Query { .. } => selected.query.push(rule.clone()),
            }
        }
    }
    selected
}

fn applies(rule: &RewriteRuleData, phase: Phase, context: &RewriteContext<'_>) -> bool {
    if let Some(dialect) = rule.action.dialect()
        && (context.operation.dialect != dialect
            || context.operation.operation != gproxy_protocol::Operation::GenerateContent)
    {
        return false;
    }
    let phase_ok = matches!(
        (rule.phase, phase),
        (RewritePhase::Both, _)
            | (RewritePhase::Request, Phase::Request)
            | (RewritePhase::Response, Phase::Response)
    );
    if !phase_ok {
        return false;
    }
    if rule
        .operation_keys
        .as_ref()
        .is_some_and(|keys| !keys.contains(&context.operation))
    {
        return false;
    }
    if let Some(matcher) = &rule.model_matcher {
        let hit = [context.upstream_model, context.requested_model]
            .into_iter()
            .flatten()
            .any(|model| matcher.is_match(model));
        if !hit {
            return false;
        }
    }
    if let Some(matcher) = &rule.header_matcher {
        let hit = context.request_headers.iter().any(|(name, value)| {
            value
                .to_str()
                .is_ok_and(|value| matcher.is_match(&format!("{name}: {value}")))
        });
        if !hit {
            return false;
        }
    }
    true
}
