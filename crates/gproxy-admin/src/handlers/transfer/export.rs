use std::collections::BTreeMap;

use bytes::Bytes;
use gproxy_store::records::{MasterKeyFingerprint, StoredSecret};
use http::{Response, StatusCode};

use crate::dto::*;
use crate::handlers::{control, default_models, identity, rules, util};
use crate::{AdminError, State, response};

pub(super) async fn run(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: ConfigurationExportRequest = util::parse(body)?;
    let snapshot = state.store().control_snapshot().await?;
    let oauth_keys = state.store().oauth_user_key_ids().await?;
    let credentials = state.store().admin_credentials().await?;
    let inventory = if request.include_secrets {
        Some(state.store().secret_inventory().await?)
    } else {
        None
    };
    let credential_secrets = secrets(inventory.as_ref().map(|value| &value.credentials));
    let user_key_secrets = secrets(inventory.as_ref().map(|value| &value.user_keys));
    let embedded_prices =
        default_models::embedded_global_rule_ids(&snapshot.price_rules, &snapshot.price_rates);
    let data = ConfigurationDataDto {
        organizations: snapshot
            .organizations
            .iter()
            .map(identity::map::organization)
            .collect(),
        teams: snapshot.teams.iter().map(identity::map::team).collect(),
        users: snapshot.users.iter().map(identity::map::user).collect(),
        providers: snapshot
            .providers
            .iter()
            .map(control::map::provider)
            .collect(),
        credentials: credentials
            .iter()
            .map(|value| ExportCredentialDto {
                config: control::map::credential(value, &[]),
                secret: credential_secrets.get(&value.id).cloned().map(Into::into),
            })
            .collect(),
        user_keys: snapshot
            .user_keys
            .iter()
            .filter(|key| !oauth_keys.contains(&key.id))
            .map(|value| ExportUserKeyDto {
                config: identity::map::user_key(value),
                digest: value.digest.clone(),
                digest_version: value.digest_version,
                secret: user_key_secrets.get(&value.id).cloned().map(Into::into),
            })
            .collect(),
        quotas: snapshot.quotas.iter().map(identity::map::quota).collect(),
        price_rules: snapshot
            .price_rules
            .iter()
            .filter(|rule| !embedded_prices.contains(&rule.id))
            .map(price_rule)
            .collect(),
        price_rates: snapshot
            .price_rates
            .iter()
            .filter(|rate| !embedded_prices.contains(&rate.rule_id))
            .map(price_rate)
            .collect(),
        routes: snapshot.routes.iter().map(control::map::route).collect(),
        route_members: snapshot
            .route_members
            .iter()
            .map(|value| {
                let mut dto = control::map::route_member(value);
                if !request.include_secrets {
                    dto.credential_id = None;
                }
                dto
            })
            .collect(),
        aliases: snapshot.aliases.iter().map(control::map::alias).collect(),
        model_aliases: snapshot
            .exposed_models
            .iter()
            .map(control::map::model_alias)
            .collect(),
        routing_rules: snapshot
            .routing_rules
            .iter()
            .map(rules::routing_dto)
            .collect::<Result<_, _>>()?,
        rule_sets: snapshot.rule_sets.iter().map(rule_set).collect(),
        rules: snapshot
            .rules
            .iter()
            .map(rules::rule_dto)
            .collect::<Result<_, _>>()?,
        provider_rule_sets: snapshot
            .provider_rule_sets
            .iter()
            .map(provider_rule_set)
            .collect(),
    };
    response::json(
        StatusCode::OK,
        &ConfigurationExportDto {
            format_version: 1,
            secrets: if request.include_secrets {
                SecretExportDto::Included
            } else {
                SecretExportDto::Omitted
            },
            source_key: inventory.as_ref().map(source_key),
            data,
        },
    )
}

fn secrets(
    values: Option<&Vec<StoredSecret>>,
) -> BTreeMap<i64, gproxy_store::records::CredentialEnvelope> {
    values
        .into_iter()
        .flatten()
        .map(|value| (value.id, value.envelope.clone()))
        .collect()
}

fn source_key(inventory: &gproxy_store::records::SecretInventory) -> ExportSourceKeyDto {
    match &inventory.fingerprint {
        MasterKeyFingerprint::Sealed(fingerprint) => ExportSourceKeyDto::Sealed {
            fingerprint: fingerprint.clone(),
        },
        MasterKeyFingerprint::Missing | MasterKeyFingerprint::Plaintext => {
            ExportSourceKeyDto::Plaintext
        }
    }
}

fn price_rule(value: &gproxy_store::records::PriceRuleRecord) -> PriceRuleDto {
    PriceRuleDto {
        id: value.id,
        provider_id: value.provider_id,
        model_pattern: value.model_pattern.clone(),
        tiers: value.tiers.clone(),
        priority: value.priority,
        enabled: value.enabled,
    }
}

fn price_rate(value: &gproxy_store::records::PriceRateRecord) -> PriceRateDto {
    PriceRateDto {
        id: value.id,
        rule_id: value.rule_id,
        metric: value.metric.clone(),
        unit_size: value.unit_size,
        price: value.price.normalize().to_string(),
        conditions: value.conditions.clone(),
        priority: value.priority,
    }
}

fn rule_set(value: &gproxy_store::records::RuleSetRecord) -> RuleSetDto {
    RuleSetDto {
        id: value.id,
        name: value.name.clone(),
        description: value.description.clone(),
        enabled: value.enabled,
    }
}

fn provider_rule_set(value: &gproxy_store::records::ProviderRuleSetRecord) -> ProviderRuleSetDto {
    ProviderRuleSetDto {
        id: value.id,
        provider_id: value.provider_id,
        rule_set_id: value.rule_set_id,
        sort_order: value.sort_order,
        enabled: value.enabled,
        inherited: false,
    }
}
