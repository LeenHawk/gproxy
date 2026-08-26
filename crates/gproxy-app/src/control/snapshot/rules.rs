use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gproxy_core::process::{CompiledRule, RuleSpec};
use gproxy_core::routing::{CompiledRoutingRule, RoutingRuleSpec};
use gproxy_store::StoreError;
use gproxy_store::records::ControlSnapshot;

type RoutingByProvider = BTreeMap<i64, Arc<[CompiledRoutingRule]>>;
type ProcessByProvider = BTreeMap<i64, Arc<[CompiledRule]>>;

pub(super) fn compile(
    stored: &ControlSnapshot,
) -> Result<(RoutingByProvider, ProcessByProvider), StoreError> {
    Ok((routing(stored)?, process(stored)?))
}

fn routing(stored: &ControlSnapshot) -> Result<RoutingByProvider, StoreError> {
    let mut grouped = BTreeMap::<i64, Vec<RoutingRuleSpec>>::new();
    for rule in &stored.routing_rules {
        grouped
            .entry(rule.provider_id)
            .or_default()
            .push(RoutingRuleSpec {
                id: rule.id,
                operation: rule.operation.clone(),
                kind: rule.kind.clone(),
                implementation: rule.implementation.clone(),
                dest_operation: rule.dest_operation.clone(),
                dest_kind: rule.dest_kind.clone(),
                sort_order: rule.sort_order,
                enabled: rule.enabled,
            });
    }
    grouped
        .into_iter()
        .map(|(provider, specs)| {
            gproxy_core::routing::compile_all(&specs)
                .map(|rules| (provider, Arc::from(rules)))
                .map_err(|message| invalid("routing_rules", message))
        })
        .collect()
}

fn process(stored: &ControlSnapshot) -> Result<ProcessByProvider, StoreError> {
    let enabled_sets = stored
        .rule_sets
        .iter()
        .filter(|set| set.enabled)
        .map(|set| set.id)
        .collect::<BTreeSet<_>>();
    let mut rules_by_set = BTreeMap::<i64, Vec<RuleSpec>>::new();
    for rule in &stored.rules {
        rules_by_set
            .entry(rule.rule_set_id)
            .or_default()
            .push(RuleSpec {
                id: rule.id,
                kind: rule.kind.clone(),
                config: rule.config.clone(),
                filter_model_pattern: rule.filter_model_pattern.clone(),
                filter_operations: rule.filter_operations.clone(),
                filter_header_pattern: rule.filter_header_pattern.clone(),
                sort_order: rule.sort_order,
                enabled: rule.enabled,
            });
    }
    let compiled_sets = rules_by_set
        .into_iter()
        .filter(|(id, _)| enabled_sets.contains(id))
        .map(|(id, specs)| {
            gproxy_core::process::compile_all(&specs)
                .map(|rules| (id, rules))
                .map_err(|message| invalid("rules", message))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let mut attachments = stored
        .provider_rule_sets
        .iter()
        .filter(|attachment| attachment.enabled && enabled_sets.contains(&attachment.rule_set_id))
        .collect::<Vec<_>>();
    attachments
        .sort_by_key(|attachment| (attachment.provider_id, attachment.sort_order, attachment.id));
    let mut by_provider = BTreeMap::<i64, Vec<CompiledRule>>::new();
    for attachment in attachments {
        if let Some(rules) = compiled_sets.get(&attachment.rule_set_id) {
            by_provider
                .entry(attachment.provider_id)
                .or_default()
                .extend(rules.iter().cloned());
        }
    }
    Ok(by_provider
        .into_iter()
        .map(|(provider, mut rules)| {
            gproxy_core::process::order_for_apply(&mut rules);
            (provider, Arc::from(rules))
        })
        .collect())
}

fn invalid(field: &'static str, message: String) -> StoreError {
    StoreError::InvalidData { field, message }
}
