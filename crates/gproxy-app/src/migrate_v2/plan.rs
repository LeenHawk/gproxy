use std::collections::BTreeSet;

use super::cipher::V2Cipher;
use super::model::SourceData;
use super::report::{ImportCount, ImportIssue};

pub(super) struct Plan {
    pub data: SourceData,
    pub counts: Vec<ImportCount>,
    pub issues: Vec<ImportIssue>,
}

pub(super) fn prepare(mut data: SourceData, cipher: &V2Cipher) -> Plan {
    let mut found = counts(&data);
    let skipped_permissions = skip_permissions(&mut data);
    let mut issues = std::mem::take(&mut data.table_issues);
    data.credentials.retain_mut(
        |credential| match cipher.open(&credential.value.stored_secret) {
            Ok(secret) => {
                credential.value.stored_secret = secret;
                true
            }
            Err(()) => {
                issues.push(issue(
                    "credentials",
                    credential.id,
                    "secret cannot be recovered with the supplied v2 master key",
                ));
                false
            }
        },
    );
    data.user_keys
        .retain_mut(|key| match cipher.user_key(&key.value.stored_key) {
            Ok(api_key) if !api_key.is_empty() => {
                key.value.stored_key = api_key;
                true
            }
            Ok(_) => {
                issues.push(issue("user_keys", key.id, "recovered API key is empty"));
                false
            }
            Err(()) => {
                issues.push(issue(
                    "user_keys",
                    key.id,
                    "API key cannot be recovered with the supplied v2 master key",
                ));
                false
            }
        });
    super::tombstone::prepare(&mut data, &mut issues);
    found.extend([
        (
            "usage_provider_tombstones",
            data.usage_tombstone_providers.len(),
        ),
        (
            "usage_credential_tombstones",
            data.usage_tombstone_credentials.len(),
        ),
    ]);
    loop {
        super::validate::run(&data, &mut issues);
        if !prune(&mut data, &issues) {
            break;
        }
    }
    let failed = issues
        .iter()
        .map(|issue| (issue.entity, issue.row.clone()))
        .collect::<BTreeSet<_>>();
    let counts = found
        .into_iter()
        .map(|(entity, found)| ImportCount {
            entity,
            found,
            importable: found
                .saturating_sub(if entity == "route_permissions" {
                    skipped_permissions
                } else {
                    0
                })
                .saturating_sub(
                    failed
                        .iter()
                        .filter(|(failed_entity, _)| *failed_entity == entity)
                        .count(),
                ),
            imported: 0,
        })
        .collect();
    Plan {
        data,
        counts,
        issues,
    }
}

fn skip_permissions(data: &mut SourceData) -> usize {
    let organizations = data
        .organizations
        .iter()
        .map(|row| row.id)
        .collect::<BTreeSet<_>>();
    let teams = data.teams.iter().map(|row| row.id).collect::<BTreeSet<_>>();
    let users = data.users.iter().map(|row| row.id).collect::<BTreeSet<_>>();
    let found = data.permissions.len();
    data.permissions.retain(|row| {
        let permission = &row.value;
        permission.route_pattern == "*"
            && match permission.scope.as_str() {
                "org" => organizations.contains(&permission.scope_id),
                "team" => teams.contains(&permission.scope_id),
                "user" => users.contains(&permission.scope_id),
                _ => false,
            }
    });
    let skipped = found - data.permissions.len();
    if skipped > 0 {
        data.skipped.push(super::report::SkippedTable {
            table: "route_permissions".into(),
            rows: skipped as u64,
            reason: "unsupported route grants or grants with missing subjects are not migrated",
        });
    }
    skipped
}

fn prune(data: &mut SourceData, issues: &[ImportIssue]) -> bool {
    let failed = issues
        .iter()
        .filter_map(|issue| {
            issue
                .row
                .strip_prefix("id=")?
                .parse()
                .ok()
                .map(|id| (issue.entity, id))
        })
        .collect::<BTreeSet<_>>();
    let mut removed = false;
    macro_rules! retain {
        ($field:ident, $entity:literal) => {
            let before = data.$field.len();
            data.$field
                .retain(|value| !failed.contains(&($entity, value.id)));
            removed |= before != data.$field.len();
        };
    }
    retain!(organizations, "organizations");
    retain!(teams, "teams");
    retain!(users, "users");
    retain!(user_keys, "user_keys");
    retain!(providers, "providers");
    retain!(credentials, "credentials");
    retain!(routes, "routes");
    retain!(route_members, "route_members");
    retain!(aliases, "aliases");
    retain!(quotas, "quotas");
    retain!(permissions, "route_permissions");
    retain!(price_rules, "price_rules");
    retain!(price_rates, "price_rates");
    retain!(routing_rules, "routing_rules");
    retain!(rule_sets, "rule_sets");
    retain!(rules, "rules");
    retain!(provider_rule_sets, "provider_rule_sets");
    retain!(settings, "instance_settings");
    retain!(usage, "usage");
    removed
}

fn counts(data: &SourceData) -> Vec<(&'static str, usize)> {
    vec![
        ("organizations", data.organizations.len()),
        ("teams", data.teams.len()),
        ("users", data.users.len()),
        ("user_keys", data.user_keys.len()),
        ("providers", data.providers.len()),
        ("credentials", data.credentials.len()),
        ("routes", data.routes.len()),
        ("route_members", data.route_members.len()),
        ("aliases", data.aliases.len()),
        ("quotas", data.quotas.len()),
        ("route_permissions", data.permissions.len()),
        ("price_rules", data.price_rules.len()),
        ("price_rates", data.price_rates.len()),
        ("routing_rules", data.routing_rules.len()),
        ("rule_sets", data.rule_sets.len()),
        ("rules", data.rules.len()),
        ("provider_rule_sets", data.provider_rule_sets.len()),
        ("instance_settings", data.settings.len()),
        ("usage", data.usage.len()),
        ("downstream_requests", data.downstream_requests),
        ("upstream_requests", data.upstream_requests),
    ]
}

pub(super) fn issue(entity: &'static str, id: i64, reason: impl Into<String>) -> ImportIssue {
    ImportIssue {
        entity,
        row: format!("id={id}"),
        reason: reason.into(),
    }
}
