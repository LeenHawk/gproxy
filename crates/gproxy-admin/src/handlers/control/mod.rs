mod inputs;
mod map;
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
        let health = state
            .store()
            .credential_health()
            .await?
            .into_iter()
            .map(|health| (health.credential_id, health))
            .collect::<std::collections::BTreeMap<_, _>>();
        let values = records
            .iter()
            .map(|credential| {
                let current = health
                    .get(&credential.id)
                    .filter(|health| health.credential_version == credential.version);
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
