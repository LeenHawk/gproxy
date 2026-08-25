use bytes::Bytes;
use http::Response;

use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State};

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let id = match entity {
        Entity::Organizations => {
            state
                .store()
                .insert_organization(&super::inputs::organization(util::parse(body)?)?)
                .await?
        }
        Entity::Teams => {
            let input = super::inputs::team(util::parse(body)?)?;
            super::validators::organization(state, input.organization_id).await?;
            state.store().insert_team(&input).await?
        }
        Entity::Users => {
            let input = super::inputs::user(util::parse(body)?)?;
            super::validators::user_scopes(state, &input).await?;
            state.store().insert_user(&input).await?
        }
        Entity::Permissions => {
            let input = super::inputs::permission(util::parse(body)?)?;
            super::validators::permission(state, &input).await?;
            state.store().insert_permission(&input).await?
        }
        Entity::RateLimits => {
            let input = super::inputs::rate_limit(util::parse(body)?)?;
            super::validators::rate_limit(state, &input).await?;
            state.store().insert_rate_limit(&input).await?
        }
        Entity::Quotas => {
            let input = super::inputs::quota(util::parse(body)?)?;
            super::validators::quota(state, &input).await?;
            state.store().insert_quota(&input).await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::created(state, id).await
}

pub(super) async fn update(
    state: &impl State,
    entity: Entity,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let applied = match entity {
        Entity::Organizations => {
            state
                .store()
                .update_organization(id, &super::inputs::organization(util::parse(body)?)?)
                .await?
        }
        Entity::Teams => {
            let input = super::inputs::team(util::parse(body)?)?;
            super::validators::organization(state, input.organization_id).await?;
            state.store().update_team(id, &input).await?
        }
        Entity::Users => {
            let input = super::inputs::user(util::parse(body)?)?;
            super::validators::user_scopes(state, &input).await?;
            state.store().update_user(id, &input).await?
        }
        Entity::Permissions => {
            let input = super::inputs::permission(util::parse(body)?)?;
            super::validators::permission(state, &input).await?;
            state.store().update_permission(id, &input).await?
        }
        Entity::RateLimits => {
            let input = super::inputs::rate_limit(util::parse(body)?)?;
            super::validators::rate_limit_update(state, id, &input).await?;
            state.store().update_rate_limit(id, &input).await?
        }
        Entity::Quotas => {
            let input = super::inputs::quota(util::parse(body)?)?;
            super::validators::quota_update(state, id, &input).await?;
            state.store().update_quota(id, &input).await?
        }
        _ => return Err(AdminError::NotFound),
    };
    util::updated(state, applied).await
}
