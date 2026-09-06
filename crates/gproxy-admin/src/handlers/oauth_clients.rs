use bytes::Bytes;
use gproxy_store::records::{OAuthClientInput, OAuthClientRecord};
use http::{Response, StatusCode};

use super::util;
use crate::dto::{OAuthClientDto, OAuthClientWriteRequest};
use crate::{AdminError, State, response};

pub(super) async fn list(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let clients = state
        .store()
        .oauth_clients()
        .await?
        .into_iter()
        .map(dto)
        .collect::<Vec<_>>();
    response::json(StatusCode::OK, &clients)
}

pub(super) async fn create(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let input = input(util::parse(body)?)?;
    if let Some(current) = state.store().oauth_client(&input.client_id).await? {
        if current.deleted_at.is_some() {
            state
                .store()
                .update_oauth_client(current.id, &input, None, crate::auth::now()?)
                .await?;
            return util::created(state, current.id).await;
        }
        return Err(AdminError::BadRequest(
            "client_id is already registered".into(),
        ));
    }
    let id = state.store().insert_oauth_client(&input).await?;
    util::created(state, id).await
}

pub(super) async fn update(
    state: &impl State,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let input = input(util::parse(body)?)?;
    let current = find(state, id).await?;
    if current.client_id != input.client_id {
        return Err(AdminError::BadRequest("client_id cannot be changed".into()));
    }
    state
        .store()
        .update_oauth_client(id, &input, None, crate::auth::now()?)
        .await?;
    util::updated(state, true).await
}

pub(super) async fn delete(state: &impl State, id: i64) -> Result<Response<Bytes>, AdminError> {
    let current = find(state, id).await?;
    let input = OAuthClientInput {
        client_id: current.client_id,
        name: current.name,
        redirect_uris: current.redirect_uris,
        enabled: false,
    };
    let now = crate::auth::now()?;
    state
        .store()
        .update_oauth_client(id, &input, Some(now), now)
        .await?;
    state.reload().await?;
    Ok(response::empty(StatusCode::NO_CONTENT))
}

async fn find(state: &impl State, id: i64) -> Result<OAuthClientRecord, AdminError> {
    state
        .store()
        .oauth_clients()
        .await?
        .into_iter()
        .find(|client| client.id == id)
        .ok_or(AdminError::NotFound)
}

fn input(value: OAuthClientWriteRequest) -> Result<OAuthClientInput, AdminError> {
    let client_id = value.client_id.trim();
    let name = value.name.trim();
    if client_id.is_empty()
        || client_id.len() > 128
        || !client_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._".contains(&byte))
        || name.is_empty()
        || name.len() > 128
        || name.chars().any(char::is_control)
    {
        return Err(AdminError::BadRequest("invalid client_id or name".into()));
    }
    let mut redirects = value
        .redirect_uris
        .into_iter()
        .map(|uri| uri.trim().to_owned())
        .collect::<Vec<_>>();
    if redirects.len() > 32
        || redirects
            .iter()
            .any(|uri| uri.len() > 2048 || !gproxy_channel_api::valid_oauth_redirect(uri))
    {
        return Err(AdminError::BadRequest(
            "redirects must be HTTPS URLs or loopback HTTP URLs without fragments or userinfo"
                .into(),
        ));
    }
    redirects.sort();
    redirects.dedup();
    Ok(OAuthClientInput {
        client_id: client_id.into(),
        name: name.into(),
        redirect_uris: redirects,
        enabled: value.enabled,
    })
}

fn dto(value: OAuthClientRecord) -> OAuthClientDto {
    OAuthClientDto {
        id: value.id,
        client_id: value.client_id,
        name: value.name,
        redirect_uris: value.redirect_uris,
        enabled: value.enabled,
    }
}
