use std::collections::BTreeSet;

use gproxy_store::records::{ProviderInput, RouteStrategy};

use super::model::{Legacy, Permission, SourceData};
use super::plan::issue;
use super::report::{ImportIssue, SkippedTable};

pub(super) fn route_strategy(
    value: &str,
    id: i64,
    name: &str,
    notices: &mut Vec<ImportIssue>,
) -> RouteStrategy {
    if let Ok(strategy) = value.parse() {
        return strategy;
    }
    let strategy = if value == "least_latency" {
        RouteStrategy::RoundRobin
    } else {
        RouteStrategy::Failover
    };
    notices.push(issue(
        "routes",
        id,
        format!(
            "name={name:?}: strategy {value:?} is not supported; using {}",
            strategy.as_str()
        ),
    ));
    strategy
}

pub(super) fn normalize(data: &mut SourceData) -> usize {
    for provider in &mut data.providers {
        let strategy = &mut provider.value.credential_strategy;
        if !matches!(strategy.as_str(), "round_robin" | "sticky") {
            data.notices.push(issue(
                "providers",
                provider.id,
                format!("name={:?}: credential_strategy {strategy:?} mapped to round_robin, matching v2's non-sticky behavior", provider.value.name),
            ));
            *strategy = "round_robin".into();
        }
    }
    let subjects = data
        .organizations
        .iter()
        .map(|row| ("org", row.id))
        .chain(data.teams.iter().map(|row| ("team", row.id)))
        .chain(data.users.iter().map(|row| ("user", row.id)))
        .collect::<BTreeSet<_>>();
    let found = data.permissions.len();
    data.permissions.retain(|row| {
        let permission = &row.value;
        let valid = subjects.contains(&(permission.scope.as_str(), permission.scope_id))
            && !permission.route_pattern.trim().is_empty();
        let detail = format!("scope={} scope_id={} pattern={:?}", permission.scope, permission.scope_id, permission.route_pattern);
        if !valid {
            data.notices.push(issue("route_permissions", row.id, format!("{detail}: skipped; missing subject, unknown scope, or empty pattern")));
        } else if permission.route_pattern != "*" {
            let patterns = permission_patterns(permission, &data.providers);
            data.notices.push(issue("route_permissions", row.id, format!("{detail}: imported as public model patterns {patterns:?}; provider-wide access is not granted")));
        }
        valid
    });
    let skipped = found - data.permissions.len();
    if skipped > 0 {
        data.skipped.push(SkippedTable {
            table: "route_permissions".into(),
            rows: skipped as u64,
            reason: "grants with missing subjects, unknown scopes, or empty patterns are not migrated; see row details",
        });
    }
    skipped
}

pub(super) fn permission_patterns(
    permission: &Permission,
    providers: &[Legacy<ProviderInput>],
) -> Vec<Option<String>> {
    if permission.route_pattern == "*" {
        return vec![None];
    }
    let mut patterns = BTreeSet::from([Some(permission.route_pattern.clone())]);
    for provider in providers {
        if crate::model_pattern::matches(&permission.route_pattern, &provider.value.name) {
            patterns.insert(Some(format!("{}/*", provider.value.name)));
        }
    }
    patterns.into_iter().collect()
}
