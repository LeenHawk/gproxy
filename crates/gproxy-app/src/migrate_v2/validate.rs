use std::collections::{BTreeMap, BTreeSet};

use super::model::{Legacy, SourceData};
use super::plan::issue;
use super::report::ImportIssue;

pub(super) fn run(data: &SourceData, issues: &mut Vec<ImportIssue>) {
    let organizations = ids(&data.organizations);
    let teams = ids(&data.teams);
    let users = ids(&data.users);
    let mut providers = ids(&data.providers);
    providers.extend(ids(&data.usage_tombstone_providers));
    let mut credentials = ids(&data.credentials);
    credentials.extend(ids(&data.usage_tombstone_credentials));
    let routes = ids(&data.routes);
    let price_rules = ids(&data.price_rules);
    let rule_sets = ids(&data.rule_sets);
    let provider_names = data
        .providers
        .iter()
        .map(|provider| (provider.value.name.as_str(), provider.id))
        .collect::<BTreeMap<_, _>>();

    for value in &data.teams {
        require(
            issues,
            "teams",
            value.id,
            &organizations,
            value.value.organization_id,
            "organization",
        );
    }
    for value in &data.users {
        optional(
            issues,
            "users",
            value.id,
            &organizations,
            value.value.organization_id,
            "organization",
        );
        optional(
            issues,
            "users",
            value.id,
            &teams,
            value.value.team_id,
            "team",
        );
    }
    for value in &data.user_keys {
        require(
            issues,
            "user_keys",
            value.id,
            &users,
            value.value.user_id,
            "user",
        );
    }
    for value in &data.credentials {
        require(
            issues,
            "credentials",
            value.id,
            &providers,
            value.value.provider_id,
            "provider",
        );
        nonnegative(
            issues,
            "credentials",
            value.id,
            value.value.weight,
            "weight",
        );
        optional_nonnegative(
            issues,
            "credentials",
            value.id,
            value.value.rpm_limit,
            "rpm_limit",
        );
        optional_nonnegative(
            issues,
            "credentials",
            value.id,
            value.value.tpm_limit,
            "tpm_limit",
        );
    }
    for value in &data.route_members {
        require(
            issues,
            "route_members",
            value.id,
            &routes,
            value.value.route_id,
            "route",
        );
        require(
            issues,
            "route_members",
            value.id,
            &providers,
            value.value.provider_id,
            "provider",
        );
    }
    for value in &data.aliases {
        if value.value.provider != "*"
            && !provider_names.contains_key(value.value.provider.as_str())
        {
            issues.push(issue(
                "aliases",
                value.id,
                "references a missing provider name",
            ));
        }
    }
    for value in &data.quotas {
        let valid = match value.value.scope.as_str() {
            "org" => organizations.contains(&value.value.scope_id),
            "team" => teams.contains(&value.value.scope_id),
            "user" => users.contains(&value.value.scope_id),
            _ => false,
        };
        if !valid {
            issues.push(issue(
                "quotas",
                value.id,
                "has an unknown scope or missing subject",
            ));
        }
    }
    control::run(data, issues, &providers, &price_rules, &rule_sets);
    usage::run(
        data,
        issues,
        usage::References {
            providers: &providers,
            credentials: &credentials,
        },
    );
}

fn ids<T>(values: &[Legacy<T>]) -> BTreeSet<i64> {
    values.iter().map(|value| value.id).collect()
}
pub(super) fn require(
    issues: &mut Vec<ImportIssue>,
    entity: &'static str,
    id: i64,
    values: &BTreeSet<i64>,
    reference: i64,
    name: &str,
) {
    if !values.contains(&reference) {
        issues.push(issue(entity, id, format!("references a missing {name}")));
    }
}
pub(super) fn optional(
    issues: &mut Vec<ImportIssue>,
    entity: &'static str,
    id: i64,
    values: &BTreeSet<i64>,
    reference: Option<i64>,
    name: &str,
) {
    if reference.is_some_and(|reference| !values.contains(&reference)) {
        issues.push(issue(entity, id, format!("references a missing {name}")));
    }
}
fn nonnegative(
    issues: &mut Vec<ImportIssue>,
    entity: &'static str,
    id: i64,
    value: i64,
    name: &str,
) {
    if value < 0 {
        issues.push(issue(entity, id, format!("{name} is negative")));
    }
}
fn optional_nonnegative(
    issues: &mut Vec<ImportIssue>,
    entity: &'static str,
    id: i64,
    value: Option<i64>,
    name: &str,
) {
    if value.is_some_and(|value| value < 0) {
        issues.push(issue(entity, id, format!("{name} is negative")));
    }
}
mod control;
mod usage;
