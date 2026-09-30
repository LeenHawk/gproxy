//! v3's model aliases as v4 variants and routes.
//!
//! v3 rewrote a requested name before routing (`v3:crates/gproxy-app/src/
//! control/snapshot/resolve.rs`): a **global** alias `alias → target` renamed
//! the model everywhere, a **provider** alias only when the request had
//! reached that provider. Enabled aliases applied in `(priority, id)` order
//! and the first one for a name won.
//!
//! - A provider alias is a variant of that provider's model: the alias
//!   becomes a variant name on the `provider_models` row whose upstream name
//!   is the target, which is how v4 serves another name for the same model.
//!   The row is created when the provider had none for the target.
//! - A global alias whose target is a route (a v3 route's public name) is a
//!   second route with the same members, the way extra public names of one
//!   route already travel. One whose target is `provider/model` is a route
//!   with that single member.
//!
//! Reported: a global alias to a bare model name, which no route or
//! provider names (v3 left the provider to be picked later; v4 needs to be
//! told), one whose name a route already has, and disabled aliases, which did
//! nothing in v3.

use std::collections::{BTreeMap, BTreeSet};

use gproxy_sdk::dto::{ProviderModelDto, RouteDto, RouteMemberDto};
use serde_json::{Value, json};

use super::{Report, document, ids};

pub fn translate(
    data: &document::Data,
    kept_provider: &dyn Fn(i64) -> bool,
    provider_models: &mut Vec<ProviderModelDto>,
    routes: &mut Vec<RouteDto>,
    members: &mut Vec<RouteMemberDto>,
    report: &mut Report,
) {
    let mut aliases: Vec<&document::Alias> = Vec::new();
    for alias in &data.aliases {
        if alias.enabled {
            aliases.push(alias);
        } else {
            report.drop_row(
                "aliases",
                row(alias),
                "it was disabled, so it did nothing in v3",
            );
        }
    }
    aliases.sort_by_key(|alias| (alias.priority, alias.id));
    let provider_names: BTreeMap<&str, i64> = data
        .providers
        .iter()
        .map(|provider| (provider.name.as_str(), provider.id))
        .collect();

    let mut seen_global = BTreeSet::new();
    let mut seen_provider = BTreeSet::new();
    for alias in aliases {
        let name = alias.alias.trim();
        let target = alias.target.trim();
        if name.is_empty() || target.is_empty() || name == target {
            report.drop_row(
                "aliases",
                row(alias),
                "its name or target is empty or the same",
            );
            continue;
        }
        match alias.provider_id {
            Some(provider_id) => {
                if !kept_provider(provider_id) {
                    report.drop_row("aliases", row(alias), "its provider was left behind");
                    continue;
                }
                if !seen_provider.insert((provider_id, name.to_owned())) {
                    report.drop_row(
                        "aliases",
                        row(alias),
                        "an earlier alias of this name won in v3",
                    );
                    continue;
                }
                variant(provider_id, name, target, provider_models);
            }
            None => {
                if !seen_global.insert(name.to_owned()) {
                    report.drop_row(
                        "aliases",
                        row(alias),
                        "an earlier alias of this name won in v3",
                    );
                    continue;
                }
                if routes.iter().any(|route| route.name == name) {
                    report.drop_row(
                        "aliases",
                        row(alias),
                        "a route already answers this name, and in v4 a route is found before any alias could apply",
                    );
                    continue;
                }
                let route_id = ids::id("aliases", alias.id);
                let copied: Vec<RouteMemberDto> = if let Some(route) =
                    routes.iter().find(|route| route.name == target)
                {
                    members
                        .iter()
                        .filter(|member| member.route_id == route.id)
                        .map(|member| RouteMemberDto {
                            id: format!("{}-alias-{}", member.id, alias.id),
                            route_id: route_id.clone(),
                            ..member.clone()
                        })
                        .collect()
                } else if let Some((provider, model)) = target
                    .split_once('/')
                    .and_then(|(head, tail)| Some((*provider_names.get(head)?, tail)))
                    .filter(|(provider, model)| kept_provider(*provider) && !model.is_empty())
                {
                    vec![RouteMemberDto {
                        id: format!("{route_id}-member"),
                        route_id: route_id.clone(),
                        provider_id: ids::id("providers", provider),
                        upstream_model: model.to_owned(),
                        tier: 0,
                        weight: 100,
                        enabled: true,
                    }]
                } else {
                    report.drop_row(
                            "aliases",
                            row(alias),
                            format!(
                                "`{target}` is neither a route nor `provider/model`; v4 needs to be told which providers serve `{name}`"
                            ),
                        );
                    continue;
                };
                let template = routes.iter().find(|route| route.name == target);
                routes.push(RouteDto {
                    id: route_id,
                    name: name.to_owned(),
                    strategy: template
                        .map_or_else(|| "weighted".to_owned(), |r| r.strategy.clone()),
                    session_affinity: template.is_some_and(|r| r.session_affinity),
                    max_attempts: template.map_or(1, |r| r.max_attempts),
                    enabled: true,
                });
                members.extend(copied);
            }
        }
    }
}

fn row(alias: &document::Alias) -> String {
    format!("alias {} ({} -> {})", alias.id, alias.alias, alias.target)
}

/// Add `name` as a variant of the provider's `target` model.
fn variant(provider_id: i64, name: &str, target: &str, models: &mut Vec<ProviderModelDto>) {
    let v4_provider = ids::id("providers", provider_id);
    let index = match models
        .iter()
        .position(|model| model.provider_id == v4_provider && model.upstream_name == target)
    {
        Some(index) => index,
        None => {
            models.push(ProviderModelDto {
                has_price: None,
                id: ids::part("providers", provider_id, &format!("model-{target}")),
                provider_id: v4_provider,
                upstream_name: target.to_owned(),
                model_id: None,
                metadata: json!({}),
                enabled: true,
            });
            models.len() - 1
        }
    };
    let metadata = &mut models[index].metadata;
    if !metadata.is_object() {
        *metadata = json!({});
    }
    let variants = metadata
        .as_object_mut()
        .expect("just made an object")
        .entry("variants")
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(list) = variants.as_array_mut()
        && !list.iter().any(|existing| existing == name)
    {
        list.push(Value::from(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_aliases_are_variants_and_global_ones_are_routes() {
        let data: document::Data = serde_json::from_value(json!({
            "providers": [{"id": 1, "name": "oa", "channel": "openai", "settings": {}}],
            "aliases": [
                {"id": 1, "alias": "fast", "target": "gpt-5-mini", "provider_id": 1, "enabled": true},
                {"id": 2, "alias": "smart", "target": "claude", "enabled": true},
                {"id": 3, "alias": "direct", "target": "oa/gpt-5", "enabled": true},
                {"id": 4, "alias": "vague", "target": "gpt-5", "enabled": true},
                {"id": 5, "alias": "off", "target": "x", "enabled": false}
            ]
        }))
        .unwrap();
        let mut models = Vec::new();
        let mut routes = vec![RouteDto {
            id: "r".into(),
            name: "claude".into(),
            strategy: "failover".into(),
            session_affinity: false,
            max_attempts: 2,
            enabled: true,
        }];
        let mut members = vec![RouteMemberDto {
            id: "m".into(),
            route_id: "r".into(),
            provider_id: "v3-providers-1".into(),
            upstream_model: "claude-x".into(),
            tier: 0,
            weight: 100,
            enabled: true,
        }];
        let mut report = Report::default();
        translate(
            &data,
            &|_| true,
            &mut models,
            &mut routes,
            &mut members,
            &mut report,
        );

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].upstream_name, "gpt-5-mini");
        assert_eq!(models[0].metadata["variants"], json!(["fast"]));

        let smart = routes.iter().find(|r| r.name == "smart").unwrap();
        assert_eq!(smart.strategy, "failover");
        assert!(
            members
                .iter()
                .any(|m| m.route_id == smart.id && m.upstream_model == "claude-x")
        );
        let direct = routes.iter().find(|r| r.name == "direct").unwrap();
        assert!(
            members
                .iter()
                .any(|m| m.route_id == direct.id && m.upstream_model == "gpt-5")
        );

        assert_eq!(
            report.dropped.len(),
            2,
            "the bare target and the disabled alias"
        );
    }
}
