use bytes::Bytes;
use http::{Response, StatusCode};

use super::import_support::*;
use crate::dto::*;
use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn run(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: ConfigurationImportRequest = util::parse(body)?;
    if request.export.format_version != 1 {
        return Err(AdminError::BadRequest(
            "unsupported export format_version".into(),
        ));
    }
    let included = request.export.secrets == SecretExportDto::Included;
    let source = match (included, request.export.source_key.as_ref()) {
        (true, Some(source)) => Some(source),
        (true, None) => {
            return Err(AdminError::BadRequest(
                "secret-bearing export has no source_key".into(),
            ));
        }
        (false, None) => None,
        (false, Some(_)) => {
            return Err(AdminError::BadRequest(
                "config-only export must not declare a source_key".into(),
            ));
        }
    };
    let mut maps = IdMaps::default();
    let mut imported = 0_u64;
    let existing = state.store().control_snapshot().await?;
    let data = request.export.data;
    for value in data.organizations {
        if let Some(current) = existing
            .organizations
            .iter()
            .find(|current| current.name == value.name)
        {
            maps.organizations.insert(value.id, current.id);
            continue;
        }
        map_create(
            state,
            Entity::Organizations,
            value.id,
            &value,
            &mut maps.organizations,
        )
        .await?;
        imported += 1;
    }
    for mut value in data.teams {
        value.organization_id = mapped(&maps.organizations, value.organization_id)?;
        if let Some(current) = existing.teams.iter().find(|current| {
            current.organization_id == value.organization_id && current.name == value.name
        }) {
            maps.teams.insert(value.id, current.id);
            continue;
        }
        map_create(state, Entity::Teams, value.id, &value, &mut maps.teams).await?;
        imported += 1;
    }
    for mut value in data.users {
        value.organization_id = optional(&maps.organizations, value.organization_id)?;
        value.team_id = optional(&maps.teams, value.team_id)?;
        if let Some(current) = existing
            .users
            .iter()
            .find(|current| current.name == value.name && current.is_admin == value.is_admin)
        {
            maps.users.insert(value.id, current.id);
            continue;
        }
        map_create(state, Entity::Users, value.id, &value, &mut maps.users).await?;
        imported += 1;
    }
    for value in data.providers {
        map_create(
            state,
            Entity::Providers,
            value.id,
            &value,
            &mut maps.providers,
        )
        .await?;
        imported += 1;
    }
    let (credential_count, skipped_credentials) = import_credentials(
        state,
        data.credentials,
        source,
        request.source_master_key.as_deref(),
        &mut maps,
    )
    .await?;
    imported += credential_count;
    for value in data.routes {
        map_create(state, Entity::Routes, value.id, &value, &mut maps.routes).await?;
        imported += 1;
    }
    for mut value in data.route_members {
        value.route_id = mapped(&maps.routes, value.route_id)?;
        value.provider_id = mapped(&maps.providers, value.provider_id)?;
        value.credential_id = value
            .credential_id
            .and_then(|id| maps.credentials.get(&id).copied());
        create(state, Entity::RouteMembers, &value).await?;
        imported += 1;
    }
    for mut value in data.aliases {
        value.provider_id = optional(&maps.providers, value.provider_id)?;
        create(state, Entity::Aliases, &value).await?;
        imported += 1;
    }
    for mut value in data.model_aliases {
        value.route_id = mapped(&maps.routes, value.route_id)?;
        create(state, Entity::ModelAliases, &value).await?;
        imported += 1;
    }
    let (user_key_count, skipped_user_keys) = import_user_keys(
        state,
        data.user_keys,
        source,
        request.source_master_key.as_deref(),
        &mut maps,
    )
    .await?;
    imported += user_key_count;
    for mut value in data.quotas {
        let Some(id) = subject(&maps, &value.subject_kind, value.subject_id)? else {
            continue;
        };
        value.subject_id = id;
        create(state, Entity::Quotas, &value).await?;
        imported += 1;
    }
    for mut value in data.price_rules {
        value.provider_id = optional(&maps.providers, value.provider_id)?;
        map_create(
            state,
            Entity::PriceRules,
            value.id,
            &value,
            &mut maps.price_rules,
        )
        .await?;
        imported += 1;
    }
    for mut value in data.price_rates {
        value.rule_id = mapped(&maps.price_rules, value.rule_id)?;
        create(state, Entity::PriceRates, &value).await?;
        imported += 1;
    }
    for mut value in data.routing_rules {
        value.provider_id = mapped(&maps.providers, value.provider_id)?;
        create(state, Entity::RoutingRules, &value).await?;
        imported += 1;
    }
    for value in data.rule_sets {
        map_create(
            state,
            Entity::RuleSets,
            value.id,
            &value,
            &mut maps.rule_sets,
        )
        .await?;
        imported += 1;
    }
    for mut value in data.rules {
        value.rule_set_id = mapped(&maps.rule_sets, value.rule_set_id)?;
        create(state, Entity::Rules, &value).await?;
        imported += 1;
    }
    for mut value in data.provider_rule_sets {
        value.provider_id = mapped(&maps.providers, value.provider_id)?;
        value.rule_set_id = mapped(&maps.rule_sets, value.rule_set_id)?;
        create(state, Entity::ProviderRuleSets, &value).await?;
        imported += 1;
    }
    state.reload().await?;
    response::json(
        StatusCode::OK,
        &ConfigurationImportResponse {
            imported,
            skipped_credentials,
            skipped_user_keys,
        },
    )
}
