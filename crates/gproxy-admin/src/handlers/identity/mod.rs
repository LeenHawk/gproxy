mod inputs;
mod keys;
pub(super) mod map;
mod validators;
mod write;

use bytes::Bytes;
use http::{Response, StatusCode};

use crate::auth::AdminIdentity;
use crate::route::Entity;
use crate::{AdminError, State, response};

pub(super) async fn list(
    state: &impl State,
    entity: Entity,
) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    match entity {
        Entity::Organizations => response::json(
            StatusCode::OK,
            &snapshot
                .organizations
                .iter()
                .map(map::organization)
                .collect::<Vec<_>>(),
        ),
        Entity::Teams => response::json(
            StatusCode::OK,
            &snapshot.teams.iter().map(map::team).collect::<Vec<_>>(),
        ),
        Entity::Users => response::json(
            StatusCode::OK,
            &snapshot.users.iter().map(map::user).collect::<Vec<_>>(),
        ),
        Entity::UserKeys => response::json(
            StatusCode::OK,
            &snapshot
                .user_keys
                .iter()
                .map(map::user_key)
                .collect::<Vec<_>>(),
        ),
        Entity::Permissions => response::json(
            StatusCode::OK,
            &snapshot
                .permissions
                .iter()
                .map(map::permission)
                .collect::<Vec<_>>(),
        ),
        Entity::RateLimits => response::json(
            StatusCode::OK,
            &snapshot
                .rate_limits
                .iter()
                .map(map::rate_limit)
                .collect::<Vec<_>>(),
        ),
        Entity::Quotas => response::json(
            StatusCode::OK,
            &snapshot.quotas.iter().map(map::quota).collect::<Vec<_>>(),
        ),
        _ => Err(AdminError::NotFound),
    }
}

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    if matches!(entity, Entity::UserKeys) {
        keys::create(state, body).await
    } else {
        write::create(state, entity, body).await
    }
}

pub(super) async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    if matches!(entity, Entity::UserKeys) {
        keys::update(state, id, body).await
    } else {
        write::update(state, entity, id, body).await
    }
}

pub(super) async fn reveal(
    state: &impl State,
    admin: &AdminIdentity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    keys::reveal(state, admin, id).await
}

pub(super) async fn password(
    state: &impl State,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: crate::dto::UserPasswordRequest = super::util::parse(body)?;
    crate::auth::password::validate(&request.password)?;
    validators::user(state, id).await?;
    let hash = crate::auth::password::hash(&request.password)?;
    if !state.store().set_user_password(id, &hash).await? {
        return Err(AdminError::NotFound);
    }
    Ok(response::empty(StatusCode::NO_CONTENT))
}

pub(super) async fn delete(
    state: &impl State,
    entity: Entity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::Organizations => state.store().delete_organization(id).await?,
        Entity::Teams => state.store().delete_team(id).await?,
        Entity::Users => state.store().delete_user(id).await?,
        Entity::UserKeys => state.store().delete_user_key(id).await?,
        Entity::Permissions => state.store().delete_permission(id).await?,
        Entity::RateLimits => state.store().delete_rate_limit(id).await?,
        Entity::Quotas => state.store().delete_quota(id).await?,
        _ => return Err(AdminError::NotFound),
    };
    crate::handlers::util::updated(state, applied).await
}
