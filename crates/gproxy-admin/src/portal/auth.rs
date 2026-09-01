use bytes::Bytes;
use http::header::SET_COOKIE;
use http::request::Parts;
use http::{HeaderValue, Response, StatusCode};

use super::PortalIdentity;
use crate::auth::{password, session, verify_same_origin};
use crate::dto::{
    PortalContextDto, PortalLoginRequest, PortalPasswordChangeRequest, PortalSessionStatusDto,
};
use crate::{AdminError, State, response};

const COOKIE_NAME: &str = "gproxy_portal_session";
const SESSION_SECONDS: i64 = 12 * 60 * 60;

pub(super) async fn identity(
    state: &impl State,
    parts: &Parts,
) -> Result<PortalIdentity, AdminError> {
    if let Some(token) = session::cookie_token(parts, COOKIE_NAME) {
        let user = state
            .store()
            .user_for_session(&session::digest(token), session::now()?)
            .await?
            .ok_or(AdminError::Unauthorized)?;
        return Ok(PortalIdentity {
            user_id: user.id,
            user_key_id: None,
            org_id: user.organization_id,
            team_id: user.team_id,
            user_name: user.name,
            key_prefix: None,
            key_label: None,
            expires_at: None,
        });
    }
    state.portal_identity(&parts.headers)
}

pub(super) async fn login(
    state: &impl State,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    verify_same_origin(parts)?;
    let request: PortalLoginRequest = parse(body)?;
    let username = request.username.trim();
    state
        .admit_auth_attempt("portal-login-source", crate::auth::source(parts))
        .await?;
    state
        .admit_auth_attempt("portal-login-user", username)
        .await?;
    let user = state
        .store()
        .user_by_username(username)
        .await?
        .filter(|user| password::verify(&request.password, &user.password_hash))
        .ok_or(AdminError::Unauthorized)?;
    state
        .clear_auth_attempts("portal-login-user", username)
        .await?;
    let token = session::create(state, user.id).await?;
    let identity = PortalIdentity {
        user_id: user.id,
        user_key_id: None,
        org_id: user.organization_id,
        team_id: user.team_id,
        user_name: user.name,
        key_prefix: None,
        key_label: None,
        expires_at: None,
    };
    let mut response = response::json(StatusCode::OK, &context(state, &identity).await?)?;
    let secure = if parts.uri.scheme_str() == Some("https")
        || parts
            .headers
            .get("x-forwarded-proto")
            .and_then(|value| value.to_str().ok())
            == Some("https")
    {
        "; Secure"
    } else {
        ""
    };
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/; Max-Age={SESSION_SECONDS}{secure}"
        ))
        .map_err(|error| AdminError::Internal(error.to_string()))?,
    );
    response::no_store(&mut response);
    Ok(response)
}

pub(super) async fn status(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    let user = match identity(state, parts).await {
        Ok(identity) => Some(context(state, &identity).await?),
        Err(AdminError::Unauthorized) => None,
        Err(error) => return Err(error),
    };
    response::json(StatusCode::OK, &PortalSessionStatusDto { user })
}

pub(super) async fn logout(
    state: &impl State,
    parts: &Parts,
) -> Result<Response<Bytes>, AdminError> {
    verify_same_origin(parts)?;
    if let Some(token) = session::cookie_token(parts, COOKIE_NAME) {
        state
            .store()
            .delete_user_session(&session::digest(token))
            .await?;
    }
    let mut response = response::empty(StatusCode::NO_CONTENT);
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_static(
            "gproxy_portal_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
        ),
    );
    response::no_store(&mut response);
    Ok(response)
}

pub(super) async fn change_password(
    state: &impl State,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    verify_same_origin(parts)?;
    let identity = identity(state, parts).await?;
    let request: PortalPasswordChangeRequest = parse(body)?;
    password::validate(&request.new_password)?;
    let user = state
        .store()
        .user_by_username(&identity.user_name)
        .await?
        .filter(|user| password::verify(&request.current_password, &user.password_hash))
        .ok_or(AdminError::Unauthorized)?;
    let hash = password::hash(&request.new_password)?;
    if !state.store().set_user_password(user.id, &hash).await? {
        return Err(AdminError::NotFound);
    }
    Ok(response::empty(StatusCode::NO_CONTENT))
}

async fn context(
    state: &impl State,
    identity: &PortalIdentity,
) -> Result<PortalContextDto, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    Ok(PortalContextDto {
        user_name: identity.user_name.clone(),
        key_prefix: identity.key_prefix.clone(),
        key_label: identity.key_label.clone(),
        expires_at: identity.expires_at,
        recent_requests_enabled: super::recent_requests_enabled(&snapshot.settings),
    })
}

fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, AdminError> {
    serde_json::from_slice(body).map_err(|error| AdminError::BadRequest(error.to_string()))
}
