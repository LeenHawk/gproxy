//! MIGRATE-FILE (temporary 2.x bridge, remove in 2.3): read legacy JSON tables.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::rows::authz::*;
use super::rows::identity::*;
use super::rows::provider::*;
use super::rows::routing::*;
use super::rows::settings::*;
use super::rows::transform::*;

#[derive(Default)]
pub(super) struct LegacyData {
    pub orgs: Vec<LegacyOrg>,
    pub teams: Vec<LegacyTeam>,
    pub users: Vec<LegacyUser>,
    pub user_keys: Vec<LegacyUserKey>,
    pub route_permissions: Vec<LegacyRoutePermission>,
    pub rate_limits: Vec<LegacyRateLimit>,
    pub quotas: Vec<LegacyQuota>,
    pub providers: Vec<LegacyProvider>,
    pub credentials: Vec<LegacyCredential>,
    pub provider_models: Vec<LegacyProviderModel>,
    pub price_rules: Vec<LegacyPriceRule>,
    pub routes: Vec<LegacyRoute>,
    pub route_members: Vec<LegacyRouteMember>,
    pub aliases: Vec<LegacyAlias>,
    pub routing_rules: Vec<LegacyRoutingRule>,
    pub rule_sets: Vec<LegacyRuleSet>,
    pub rules: Vec<LegacyRule>,
    pub provider_rule_sets: Vec<LegacyProviderRuleSet>,
    pub instance_settings: Vec<LegacyInstanceSettings>,
    pub consumed: Vec<PathBuf>,
}

#[derive(Deserialize)]
struct Table<T> {
    rows: Vec<T>,
}

fn load<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<Vec<T>> {
    let bytes = std::fs::read(path)
        .map_err(|e| anyhow::anyhow!("read legacy table {}: {e}", path.display()))?;
    let table: Table<T> = serde_json::from_slice(&bytes)
        .map_err(|e| anyhow::anyhow!("parse legacy table {}: {e}", path.display()))?;
    Ok(table.rows)
}

pub(super) fn read_all(root: &Path) -> anyhow::Result<LegacyData> {
    let mut data = LegacyData::default();
    macro_rules! read_table {
        ($field:ident, $name:literal) => {{
            let path = root.join($name);
            if path.exists() {
                data.$field = load(&path)?;
                data.consumed.push(path);
            }
        }};
    }
    read_table!(orgs, "orgs.json");
    read_table!(teams, "teams.json");
    read_table!(users, "users.json");
    read_table!(user_keys, "user_keys.json");
    read_table!(route_permissions, "route_permissions.json");
    read_table!(rate_limits, "rate_limits.json");
    read_table!(quotas, "quotas.json");
    read_table!(providers, "providers.json");
    read_table!(credentials, "credentials.json");
    read_table!(provider_models, "provider_models.json");
    read_table!(price_rules, "price_rules.json");
    read_table!(routes, "routes.json");
    read_table!(route_members, "route_members.json");
    read_table!(aliases, "aliases.json");
    read_table!(routing_rules, "routing_rules.json");
    read_table!(rule_sets, "rule_sets.json");
    read_table!(rules, "rules.json");
    read_table!(provider_rule_sets, "provider_rule_sets.json");
    read_table!(instance_settings, "instance_settings.json");
    Ok(data)
}

pub(super) fn legacy_tables_present(root: &Path) -> bool {
    ["providers.json", "users.json", "routes.json"]
        .iter()
        .any(|name| root.join(name).is_file())
}

pub(super) fn backup_consumed(paths: &[PathBuf]) -> anyhow::Result<()> {
    for source in paths {
        let target = source.with_extension("json.filebak");
        anyhow::ensure!(
            !target.exists(),
            "legacy backup already exists: {}",
            target.display()
        );
    }
    let mut moved = Vec::new();
    for source in paths {
        let target = source.with_extension("json.filebak");
        if let Err(error) = std::fs::rename(source, &target) {
            for (from, to) in moved.into_iter().rev() {
                let _ = std::fs::rename(to, from);
            }
            anyhow::bail!(
                "rename {} to {}: {error}",
                source.display(),
                target.display()
            );
        }
        moved.push((source.clone(), target));
    }
    Ok(())
}
