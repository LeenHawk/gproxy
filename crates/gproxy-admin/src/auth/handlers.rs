use bytes::Bytes;
use http::request::Parts;
use http::{HeaderValue, Method, Response, StatusCode};

use super::{password, session};
use crate::dto::{AdminIdentityDto, AuthResponse, LoginRequest, SessionStatusDto, SetupRequest};
use crate::{AdminError, State, response};

pub(crate) async fn dispatch_public(
    state: &impl State,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Response<Bytes>, AdminError>> {
    match (&parts.method, parts.uri.path()) {
        (&Method::GET, "/admin/session") => Some(status(state, parts).await),
        (&Method::POST, "/admin/setup") => Some(setup(state, parts, body).await),
        (&Method::POST, "/admin/login") => Some(login(state, parts, body).await),
        (&Method::POST, "/admin/logout") => Some(logout(state, parts).await),
        _ => None,
    }
}

async fn status(state: &impl State, parts: &Parts) -> Result<Response<Bytes>, AdminError> {
    let setup_required = !state.store().has_admin_accounts().await?;
    let user = if setup_required {
        None
    } else {
        match session::authenticate(state, parts).await {
            Ok(identity) => Some(dto_identity(identity)),
            Err(AdminError::Unauthorized) => None,
            Err(error) => return Err(error),
        }
    };
    response::json(
        StatusCode::OK,
        &SessionStatusDto {
            setup_required,
            user,
        },
    )
}

async fn setup(
    state: &impl State,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    super::csrf::verify_same_origin(parts)?;
    state
        .admit_auth_attempt("setup-source", super::source(parts))
        .await?;
    if state.store().has_admin_accounts().await? {
        return Err(AdminError::Conflict(
            "admin setup is already complete".into(),
        ));
    }
    let request: SetupRequest = parse(body)?;
    let username = username(request.username)?;
    password::validate(&request.password)?;
    state.admit_auth_attempt("setup", &username).await?;
    let hash = password::hash(&request.password)?;
    let at = session::now()?;
    let id = state
        .store()
        .create_first_admin(&username, &hash, at)
        .await?
        .ok_or_else(|| AdminError::Conflict("admin setup is already complete".into()))?;
    let token = session::create(state, id).await?;
    auth_response(id, username, &token)
}

async fn login(
    state: &impl State,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    super::csrf::verify_same_origin(parts)?;
    let request: LoginRequest = parse(body)?;
    state
        .admit_auth_attempt("login-source", super::source(parts))
        .await?;
    let account = state
        .store()
        .admin_by_username(request.username.trim())
        .await?
        .filter(|account| account.enabled)
        .ok_or(AdminError::Unauthorized)?;
    state
        .admit_auth_attempt("login-account", request.username.trim())
        .await?;
    if !password::verify(&request.password, &account.password_hash) {
        return Err(AdminError::Unauthorized);
    }
    state
        .clear_auth_attempts("login-account", request.username.trim())
        .await?;
    let token = session::create(state, account.id).await?;
    auth_response(account.id, account.username, &token)
}

async fn logout(state: &impl State, parts: &Parts) -> Result<Response<Bytes>, AdminError> {
    super::csrf::verify_same_origin(parts)?;
    session::revoke(state, parts).await?;
    let mut response = response::empty(StatusCode::NO_CONTENT);
    response.headers_mut().insert(
        http::header::SET_COOKIE,
        HeaderValue::from_static(session::clear_cookie()),
    );
    response::no_store(&mut response);
    Ok(response)
}

fn auth_response(id: i64, username: String, token: &str) -> Result<Response<Bytes>, AdminError> {
    let mut response = response::json(
        StatusCode::OK,
        &AuthResponse {
            user: AdminIdentityDto { id, username },
        },
    )?;
    response.headers_mut().insert(
        http::header::SET_COOKIE,
        HeaderValue::from_str(&session::set_cookie(token))
            .map_err(|error| AdminError::Internal(error.to_string()))?,
    );
    Ok(response)
}

fn dto_identity(identity: session::AdminIdentity) -> AdminIdentityDto {
    AdminIdentityDto {
        id: identity.id,
        username: identity.username,
    }
}

fn parse<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, AdminError> {
    serde_json::from_slice(body).map_err(|error| AdminError::BadRequest(error.to_string()))
}

fn username(value: String) -> Result<String, AdminError> {
    let value = value.trim();
    if value.is_empty() {
        Err(AdminError::BadRequest("username must not be blank".into()))
    } else {
        Ok(value.to_owned())
    }
}
