use std::collections::BTreeMap;

use gproxy_channel_api::CallerIdentity;
use gproxy_store::records::{AliasRecord, ControlSnapshot};

use super::types::{CompiledRoute, KeyIdentity};

pub(super) struct ExposedIndex {
    pub routes: BTreeMap<String, i64>,
    pub namespaces: BTreeMap<String, BTreeMap<String, i64>>,
    pub catalogue: BTreeMap<String, gproxy_core::ExposedModel>,
    pub variants: BTreeMap<String, String>,
}

pub(super) fn exposed(
    stored: &ControlSnapshot,
    routes: &BTreeMap<i64, CompiledRoute>,
) -> Result<ExposedIndex, gproxy_store::StoreError> {
    let available = stored
        .exposed_models
        .iter()
        .filter(|model| model.enabled && routes.contains_key(&model.route_id))
        .collect::<Vec<_>>();
    let base_names = available
        .iter()
        .map(|model| model.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut route_index = BTreeMap::new();
    let mut namespaces: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut catalogue = BTreeMap::new();
    let mut variants = BTreeMap::new();
    let provider_models = super::capability::by_provider_model(&stored.provider_models);
    for model in available {
        let folded = match routes.get(&model.route_id) {
            Some(route) => super::capability::fold(route, &model.name, &provider_models)?,
            None => super::capability::Folded::default(),
        };
        route_index.insert(model.name.clone(), model.route_id);
        if let Some((namespace, local_name)) = model.name.split_once('/')
            && !namespace.is_empty()
            && !local_name.is_empty()
        {
            namespaces
                .entry(namespace.to_ascii_lowercase())
                .or_default()
                .insert(local_name.to_owned(), model.route_id);
        }
        let parsed = gproxy_store::records::parse_model_variants(folded.variants.as_ref())
            .map_err(|message| gproxy_store::StoreError::InvalidData {
                field: "model variants",
                message: format!("{}: {message}", model.name),
            })?;
        if parsed.expose_base {
            catalogue.insert(
                model.name.clone(),
                catalogue_entry(&folded, model.name.clone()),
            );
        }
        for variant in parsed.names {
            if variant == model.name {
                continue;
            }
            if base_names.contains(variant.as_str()) || variants.contains_key(&variant) {
                return Err(gproxy_store::StoreError::InvalidData {
                    field: "model variants",
                    message: format!("duplicate exposed model `{variant}`"),
                });
            }
            variants.insert(variant.clone(), model.name.clone());
            catalogue.insert(variant.clone(), catalogue_entry(&folded, variant));
        }
    }
    Ok(ExposedIndex {
        routes: route_index,
        namespaces,
        catalogue,
        variants,
    })
}

fn catalogue_entry(folded: &super::capability::Folded, id: String) -> gproxy_core::ExposedModel {
    gproxy_core::ExposedModel {
        id,
        display_name: folded.display_name.clone(),
        context_window: folded.context_window,
        max_output_tokens: folded.max_output_tokens,
        thinking_supported: folded.thinking_supported,
        thinking_adaptive_supported: folded.thinking_adaptive_supported,
        thinking_enabled_supported: folded.thinking_enabled_supported,
    }
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
