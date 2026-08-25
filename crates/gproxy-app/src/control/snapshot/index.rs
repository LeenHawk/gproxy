use std::collections::BTreeMap;

use gproxy_channel_api::CallerIdentity;
use gproxy_store::records::{AliasRecord, ControlSnapshot};

use super::types::{CompiledRoute, KeyIdentity};

pub(super) fn exposed(
    stored: &ControlSnapshot,
    routes: &BTreeMap<i64, CompiledRoute>,
) -> (
    BTreeMap<String, i64>,
    BTreeMap<String, BTreeMap<String, i64>>,
) {
    let mut exposed = BTreeMap::new();
    let mut namespaces: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    for model in stored
        .exposed_models
        .iter()
        .filter(|model| model.enabled && routes.contains_key(&model.route_id))
    {
        exposed.insert(model.name.clone(), model.route_id);
        if let Some((namespace, local_name)) = model.name.split_once('/')
            && !namespace.is_empty()
            && !local_name.is_empty()
        {
            namespaces
                .entry(namespace.to_ascii_lowercase())
                .or_default()
                .insert(local_name.to_owned(), model.route_id);
        }
    }
    (exposed, namespaces)
}

pub(super) fn aliases(
    aliases: &[AliasRecord],
) -> (
    BTreeMap<String, String>,
    BTreeMap<i64, BTreeMap<String, String>>,
) {
    let mut enabled = aliases
        .iter()
        .filter(|alias| alias.enabled)
        .collect::<Vec<_>>();
    enabled.sort_by_key(|alias| (alias.priority, alias.id));
    let mut global = BTreeMap::new();
    let mut providers: BTreeMap<i64, BTreeMap<String, String>> = BTreeMap::new();
    for alias in enabled {
        match alias.provider_id {
            Some(provider) => {
                providers
                    .entry(provider)
                    .or_default()
                    .entry(alias.alias.clone())
                    .or_insert_with(|| alias.target.clone());
            }
            None => {
                global
                    .entry(alias.alias.clone())
                    .or_insert_with(|| alias.target.clone());
            }
        }
    }
    (global, providers)
}

pub(super) fn identities(stored: &ControlSnapshot) -> BTreeMap<(u32, Vec<u8>), KeyIdentity> {
    let organizations = stored
        .organizations
        .iter()
        .filter(|organization| organization.enabled)
        .map(|organization| organization.id)
        .collect::<std::collections::BTreeSet<_>>();
    let teams = stored
        .teams
        .iter()
        .filter(|team| team.enabled && organizations.contains(&team.organization_id))
        .map(|team| (team.id, team.organization_id))
        .collect::<BTreeMap<_, _>>();
    let users = stored
        .users
        .iter()
        .filter(|user| {
            user.enabled
                && user
                    .organization_id
                    .is_none_or(|organization| organizations.contains(&organization))
                && user.team_id.is_none_or(|team| {
                    teams.get(&team).is_some_and(|team_organization| {
                        user.organization_id == Some(*team_organization)
                    })
                })
        })
        .map(|user| (user.id, user))
        .collect::<BTreeMap<_, _>>();
    stored
        .user_keys
        .iter()
        .filter_map(|key| {
            let user = users.get(&key.user_id)?;
            (key.enabled && super::super::supported_user_key_digest(key.digest_version)).then(
                || {
                    (
                        (key.digest_version, key.digest.clone()),
                        KeyIdentity {
                            caller: CallerIdentity {
                                user_id: user.id,
                                user_key_id: key.id,
                                org_id: user.organization_id,
                                team_id: user.team_id,
                            },
                            expires_at: key.expires_at,
                        },
                    )
                },
            )
        })
        .collect()
}
