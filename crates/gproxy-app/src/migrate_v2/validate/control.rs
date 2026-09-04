use std::collections::BTreeSet;

use super::{optional, require};
use crate::migrate_v2::model::SourceData;
use crate::migrate_v2::plan::issue;
use crate::migrate_v2::report::ImportIssue;

pub(super) fn run(
    data: &SourceData,
    issues: &mut Vec<ImportIssue>,
    providers: &BTreeSet<i64>,
    price_rules: &BTreeSet<i64>,
    rule_sets: &BTreeSet<i64>,
) {
    for value in &data.price_rules {
        optional(
            issues,
            "price_rules",
            value.id,
            providers,
            value.value.provider_id,
            "provider",
        );
        if !matches!(value.value.match_type.as_str(), "exact" | "contains")
            || value.value.model_match.contains('*')
        {
            issues.push(issue(
                "price_rules",
                value.id,
                "match cannot be represented by the v3 model glob",
            ));
        }
    }
    for value in &data.price_rates {
        require(
            issues,
            "price_rates",
            value.id,
            price_rules,
            value.value.rule_id,
            "price rule",
        );
        if value.value.unit_size <= 0 {
            issues.push(issue("price_rates", value.id, "unit_size must be positive"));
        }
    }
    let mut routing_keys = BTreeSet::new();
    for value in &data.routing_rules {
        require(
            issues,
            "routing_rules",
            value.id,
            providers,
            value.value.provider_id,
            "provider",
        );
        let rule = &value.value;
        let key = (rule.provider_id, rule.operation.clone(), rule.kind.clone());
        if !routing_keys.insert(key) {
            issues.push(issue(
                "routing_rules",
                value.id,
                "duplicates a provider operation after v3 kind normalization",
            ));
        }
        let spec = gproxy_core::routing::RoutingRuleSpec {
            id: value.id,
            operation: rule.operation.clone(),
            kind: rule.kind.clone(),
            implementation: rule.implementation.clone(),
            dest_operation: rule.dest_operation.clone(),
            dest_kind: rule.dest_kind.clone(),
            sort_order: rule.sort_order,
            enabled: rule.enabled,
        };
        if let Err(reason) = gproxy_core::routing::compile(&spec) {
            issues.push(issue("routing_rules", value.id, reason));
        }
    }
    for value in &data.rules {
        require(
            issues,
            "rules",
            value.id,
            rule_sets,
            value.value.rule_set_id,
            "rule set",
        );
    }
    for value in &data.provider_rule_sets {
        require(
            issues,
            "provider_rule_sets",
            value.id,
            providers,
            value.value.provider_id,
            "provider",
        );
        require(
            issues,
            "provider_rule_sets",
            value.id,
            rule_sets,
            value.value.rule_set_id,
            "rule set",
        );
    }
    if data.settings.len() > 1 {
        for value in data.settings.iter().skip(1) {
            issues.push(issue(
                "instance_settings",
                value.id,
                "v3 has one effective instance settings record",
            ));
        }
    }
    if let Some(value) = data.settings.first() {
        if value.value.instance_name.trim().is_empty() {
            issues.push(issue(
                "instance_settings",
                value.id,
                "instance name is blank",
            ));
        }
        if value.value.file_upload_max_in_flight < 0 {
            issues.push(issue(
                "instance_settings",
                value.id,
                "file upload concurrency is negative",
            ));
        }
    }
}
