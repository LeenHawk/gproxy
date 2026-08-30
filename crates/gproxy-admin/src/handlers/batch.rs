use bytes::Bytes;
use http::{Response, StatusCode};
use serde::Serialize;

use crate::dto::{BatchActionDto, BatchItemOutcome, BatchRequest, BatchResponse};
use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn run(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: BatchRequest = util::parse(body)?;
    if request.ids.is_empty() {
        return Err(AdminError::BadRequest("batch ids must not be empty".into()));
    }
    if !supports(entity, request.action) {
        return Err(AdminError::BadRequest(
            "batch action is unavailable for this entity".into(),
        ));
    }
    let mut outcomes = Vec::with_capacity(request.ids.len());
    for id in request.ids {
        let result = apply(state, entity, request.action, id).await;
        outcomes.push(match result {
            Ok(()) => BatchItemOutcome {
                id,
                applied: true,
                status: StatusCode::OK.as_u16(),
                error: None,
            },
            Err(error) => BatchItemOutcome {
                id,
                applied: false,
                status: error.status().as_u16(),
                error: Some(error.public_message()),
            },
        });
    }
    response::json(StatusCode::OK, &BatchResponse { outcomes })
}

async fn apply(
    state: &impl State,
    entity: Entity,
    action: BatchActionDto,
    id: i64,
) -> Result<(), AdminError> {
    let response = match action {
        BatchActionDto::Delete => super::delete(state, entity, id).await?,
        BatchActionDto::Enable | BatchActionDto::Disable => {
            let body = toggle_body(state, entity, id, action == BatchActionDto::Enable).await?;
            super::update(state, entity, id, &body).await?
        }
    };
    if response.status().is_success() {
        Ok(())
    } else {
        Err(AdminError::Internal(
            "batch item returned an error response".into(),
        ))
    }
}

async fn toggle_body(
    state: &impl State,
    entity: Entity,
    id: i64,
    enabled: bool,
) -> Result<Bytes, AdminError> {
    if matches!(entity, Entity::Credentials) {
        let record = state
            .store()
            .admin_credentials()
            .await?
            .into_iter()
            .find(|record| record.id == id)
            .ok_or(AdminError::NotFound)?;
        return toggled(record, enabled);
    }
    let snapshot = state.store().control_snapshot().await?;
    let value = match entity {
        Entity::Organizations => find(&snapshot.organizations, id)?,
        Entity::Teams => find(&snapshot.teams, id)?,
        Entity::Providers => find(&snapshot.providers, id)?,
        Entity::Routes => find(&snapshot.routes, id)?,
        Entity::RouteMembers => find(&snapshot.route_members, id)?,
        Entity::Aliases => find(&snapshot.aliases, id)?,
        Entity::ModelAliases => find(&snapshot.exposed_models, id)?,
        Entity::ProviderModels => find(&snapshot.provider_models, id)?,
        Entity::Users => find(&snapshot.users, id)?,
        Entity::UserKeys => find(&snapshot.user_keys, id)?,
        Entity::Quotas => find(&snapshot.quotas, id)?,
        Entity::PriceRules => find(&snapshot.price_rules, id)?,
        Entity::RoutingRules => {
            let record = snapshot
                .routing_rules
                .iter()
                .find(|record| record.id == id)
                .ok_or(AdminError::NotFound)?;
            serde_json::to_value(super::rules::routing_dto(record)?)
                .map_err(|error| AdminError::Internal(error.to_string()))?
        }
        Entity::RuleSets => find(&snapshot.rule_sets, id)?,
        Entity::Rules => {
            let record = snapshot
                .rules
                .iter()
                .find(|record| record.id == id)
                .ok_or(AdminError::NotFound)?;
            serde_json::to_value(super::rules::rule_dto(record)?)
                .map_err(|error| AdminError::Internal(error.to_string()))?
        }
        Entity::ProviderRuleSets => find(&snapshot.provider_rule_sets, id)?,
        Entity::Credentials | Entity::Permissions | Entity::RateLimits | Entity::PriceRates => {
            return Err(AdminError::NotFound);
        }
    };
    toggled(value, enabled)
}

fn find<T: Serialize>(records: &[T], id: i64) -> Result<serde_json::Value, AdminError> {
    records
        .iter()
        .find_map(|record| {
            let value = serde_json::to_value(record).ok()?;
            (value.get("id")?.as_i64() == Some(id)).then_some(value)
        })
        .ok_or(AdminError::NotFound)
}

fn toggled(value: impl Serialize, enabled: bool) -> Result<Bytes, AdminError> {
    let mut value =
        serde_json::to_value(value).map_err(|error| AdminError::Internal(error.to_string()))?;
    value["enabled"] = enabled.into();
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| AdminError::Internal(error.to_string()))
}

fn supports(entity: Entity, action: BatchActionDto) -> bool {
    match action {
        BatchActionDto::Delete => true,
        BatchActionDto::Enable | BatchActionDto::Disable => !matches!(
            entity,
            Entity::Permissions | Entity::RateLimits | Entity::PriceRates
        ),
    }
}
