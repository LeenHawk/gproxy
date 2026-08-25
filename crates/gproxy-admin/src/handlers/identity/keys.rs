use base64::Engine as _;
use bytes::Bytes;
use gproxy_store::records::{UserKeyInput, UserKeyUpdateInput};
use http::{Response, StatusCode};

use crate::auth::{AdminIdentity, now};
use crate::dto::{
    UserKeyCreateRequest, UserKeyCreateResponse, UserKeyPrefix, UserKeyRevealResponse,
    UserKeyUpdateRequest,
};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn create(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: UserKeyCreateRequest = util::parse(body)?;
    super::validators::user(state, request.user_id).await?;
    let current_time = now()?;
    if request
        .expires_at
        .is_some_and(|expires| expires <= current_time)
    {
        return Err(AdminError::BadRequest(
            "user key expiry must be in the future".into(),
        ));
    }
    let prefix = match request.prefix {
        UserKeyPrefix::Sk => "sk",
        UserKeyPrefix::At => "at",
    };
    let api_key = generate(prefix)?;
    let (digest_version, digest) = state.digest_user_key(&api_key);
    let id = state
        .store()
        .insert_user_key(&UserKeyInput {
            user_id: request.user_id,
            digest,
            digest_version,
            prefix: api_key.chars().take(12).collect(),
            envelope: state.seal_user_key(&api_key)?,
            label: request.label,
            expires_at: request.expires_at,
            enabled: request.enabled,
        })
        .await?;
    state.reload().await?;
    response::json(
        StatusCode::CREATED,
        &UserKeyCreateResponse {
            id,
            api_key,
            prefix: prefix.into(),
        },
    )
}

pub(super) async fn update(
    state: &impl State,
    id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: UserKeyUpdateRequest = util::parse(body)?;
    let applied = state
        .store()
        .update_user_key(
            id,
            &UserKeyUpdateInput {
                label: request.label,
                expires_at: request.expires_at,
                enabled: request.enabled,
            },
        )
        .await?;
    util::updated(state, applied).await
}

pub(super) async fn reveal(
    state: &impl State,
    admin: &AdminIdentity,
    id: i64,
) -> Result<Response<Bytes>, AdminError> {
    let revealed_at = now()?;
    let api_key = state.reveal_user_key(admin.id, id, revealed_at).await?;
    response::json(
        StatusCode::OK,
        &UserKeyRevealResponse {
            id,
            api_key,
            revealed_at,
        },
    )
}

fn generate(prefix: &str) -> Result<String, AdminError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| AdminError::Internal("secure randomness unavailable".into()))?;
    Ok(format!(
        "{prefix}-gp-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    ))
}
