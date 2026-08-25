use bytes::Bytes;
use gproxy_store::records::{CredentialInput, CredentialUpdateInput};
use http::Response;

use crate::dto::CredentialWriteRequest;
use crate::handlers::util;
use crate::route::Entity;
use crate::{AdminError, State};

pub(super) async fn create(
    state: &impl State,
    entity: Entity,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let id = match entity {
        Entity::Providers => {
            let input = super::inputs::provider(state, util::parse(body)?)?;
            state.store().insert_provider(&input).await?
        }
        Entity::Credentials => {
            let request: CredentialWriteRequest = util::parse(body)?;
            super::validators::provider(state, request.provider_id).await?;
            let secret = request
                .secret
                .as_ref()
                .ok_or_else(|| AdminError::BadRequest("credential secret is required".into()))?;
            state
                .store()
                .insert_credential(&CredentialInput {
                    provider_id: request.provider_id,
                    label: request.label,
                    envelope: state.seal_credential(secret)?,
                    enabled: request.enabled,
                })
                .await?
        }
        Entity::Routes => {
            state
                .store()
                .insert_route(&super::inputs::route(util::parse(body)?)?)
                .await?
        }
        Entity::RouteMembers => {
            let input = super::inputs::route_member(util::parse(body)?)?;
            super::validators::route_member(state, &input).await?;
            state.store().insert_route_member(&input).await?
        }
        Entity::Aliases => {
            let input = super::inputs::alias(util::parse(body)?)?;
            super::validators::alias(state, &input).await?;
            state.store().insert_alias(&input).await?
        }
        Entity::ModelAliases => {
            let input = super::inputs::model_alias(util::parse(body)?)?;
            super::validators::model_alias(state, &input).await?;
            state.store().insert_exposed_model(&input).await?
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
        Entity::Providers => {
            let input = super::inputs::provider(state, util::parse(body)?)?;
            state.store().update_provider(id, &input).await?
        }
        Entity::Credentials => {
            let request: CredentialWriteRequest = util::parse(body)?;
            super::validators::provider(state, request.provider_id).await?;
            let envelope = request
                .secret
                .as_ref()
                .map(|secret| state.seal_credential(secret))
                .transpose()?;
            state
                .store()
                .update_credential(
                    id,
                    &CredentialUpdateInput {
                        provider_id: request.provider_id,
                        label: request.label,
                        envelope,
                        enabled: request.enabled,
                    },
                )
                .await?
        }
        Entity::Routes => {
            state
                .store()
                .update_route(id, &super::inputs::route(util::parse(body)?)?)
                .await?
        }
        Entity::RouteMembers => {
            let input = super::inputs::route_member(util::parse(body)?)?;
            super::validators::route_member(state, &input).await?;
            state.store().update_route_member(id, &input).await?
        }
        Entity::Aliases => {
            let input = super::inputs::alias(util::parse(body)?)?;
            super::validators::alias(state, &input).await?;
            state.store().update_alias(id, &input).await?
        }
        Entity::ModelAliases => {
            let input = super::inputs::model_alias(util::parse(body)?)?;
            super::validators::model_alias(state, &input).await?;
            state.store().update_exposed_model(id, &input).await?
        }
        _ => return Err(AdminError::NotFound),
    };
    if applied && matches!(entity, Entity::Credentials) {
        state.store().clear_credential_health(id).await?;
    }
    util::updated(state, applied).await
}
