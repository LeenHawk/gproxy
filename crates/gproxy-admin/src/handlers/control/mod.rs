mod inputs;
pub(super) mod map;
pub(super) mod validators;
mod write;

use bytes::Bytes;
use http::{Response, StatusCode};

use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn list(
    state: &impl State,
    entity: Entity,
) -> Result<Response<Bytes>, AdminError> {
    if matches!(entity, Entity::Credentials) {
        let records = state.store().admin_credentials().await?;
        let health = state.store().credential_health().await?.into_iter().fold(
            std::collections::BTreeMap::<_, Vec<_>>::new(),
            |mut grouped, health| {
                grouped
                    .entry(health.credential_id)
                    .or_default()
                    .push(health);
                grouped
            },
        );
        let values = records
            .iter()
            .map(|credential| {
                let current = health
                    .get(&credential.id)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                map::credential(credential, current)
            })
            .collect::<Vec<_>>();
        return response::json(StatusCode::OK, &values);
    }
    let snapshot = state.store().control_snapshot().await?;
    match entity {
        Entity::Providers => response::json(
            StatusCode::OK,
            &snapshot
                .providers
                .iter()
                .map(map::provider)
                .collect::<Vec<_>>(),
        ),
        Entity::Routes => response::json(
            StatusCode::OK,
            &snapshot.routes.iter().map(map::route).collect::<Vec<_>>(),
        ),
        Entity::RouteMembers => response::json(
            StatusCode::OK,
            &snapshot
                .route_members
                .iter()
                .map(map::route_member)
                .collect::<Vec<_>>(),
        ),
        Entity::Aliases => response::json(
            StatusCode::OK,
            &snapshot.aliases.iter().map(map::alias).collect::<Vec<_>>(),
        ),
        Entity::ModelAliases => response::json(
            StatusCode::OK,
            &snapshot
                .exposed_models
                .iter()
                .map(map::model_alias)
                .collect::<Vec<_>>(),
        ),
        Entity::ProviderModels => response::json(
            StatusCode::OK,
            &snapshot
                .provider_models
                .iter()
                .map(map::provider_model)
                .collect::<Vec<_>>(),
        ),
        _ => Err(AdminError::NotFound),
    }
}

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    write::create(state, entity, body).await
}

pub(super) async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    write::update(state, entity, id, body).await
}

pub(super) async fn delete(
    state: &impl State,
    entity: Entity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::Providers => state.store().delete_provider(id).await?,
        Entity::Credentials => state.store().delete_credential(id).await?,
        Entity::Routes => state.store().delete_route(id).await?,
        Entity::RouteMembers => state.store().delete_route_member(id).await?,
        Entity::Aliases => state.store().delete_alias(id).await?,
        Entity::ModelAliases => state.store().delete_exposed_model(id).await?,
        Entity::ProviderModels => state.store().delete_provider_model(id).await?,
        _ => return Err(AdminError::NotFound),
    };
    crate::handlers::util::updated(state, applied).await
}

pub(super) async fn credential_health_reset(
    state: &impl State,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    write::credential_health_reset(state, id).await
}
