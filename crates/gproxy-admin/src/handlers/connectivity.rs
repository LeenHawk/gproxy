use bytes::Bytes;
use http::{Response, StatusCode};

use crate::dto::ConnectivityTestRequest;
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn test(state: &impl State, body: &Bytes) -> Result<Response<Bytes>, AdminError> {
    let request: ConnectivityTestRequest = util::parse(body)?;
    validate(&request)?;
    response::json(StatusCode::OK, &state.connectivity_test(&request).await?)
}

pub(super) async fn model_test(
    state: &impl State,
    actor_user_id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: crate::dto::ModelTestRequest = util::parse(body)?;
    if request.model_id.trim().is_empty() {
        return Err(AdminError::BadRequest("model id must not be blank".into()));
    }
    response::json(
        StatusCode::OK,
        &state.test_model(actor_user_id, &request).await?,
    )
}

pub(super) async fn quota_probe(
    state: &impl State,
    credential_id: i64,
    parts: &http::request::Parts,
) -> Result<Response<Bytes>, AdminError> {
    let query = util::query(parts);
    let force = util::value(&query, "force") == Some("true");
    response::json(
        StatusCode::OK,
        &state.quota_probe(credential_id, force).await?,
    )
}

pub(super) async fn quota_reset(
    state: &impl State,
    credential_id: i64,
) -> Result<Response<Bytes>, AdminError> {
    response::json(StatusCode::OK, &state.quota_reset(credential_id).await?)
}

pub(super) async fn model_discover(
    state: &impl State,
    actor_user_id: i64,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: crate::dto::ModelDiscoverRequest = util::parse(body)?;
    response::json(
        StatusCode::OK,
        &state.discover_models(actor_user_id, &request).await?,
    )
}

fn validate(request: &ConnectivityTestRequest) -> Result<(), AdminError> {
    let valid = match request.scope {
        crate::dto::ConnectivityScopeDto::Global => {
            request.provider_id.is_none()
                && request.credential_id.is_none()
                && request.proxy_url.is_none()
        }
        crate::dto::ConnectivityScopeDto::Proxy => {
            request.provider_id.is_none()
                && request.credential_id.is_none()
                && request
                    .proxy_url
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        }
        crate::dto::ConnectivityScopeDto::Provider => {
            request.provider_id.is_some()
                && request.credential_id.is_none()
                && request.proxy_url.is_none()
        }
        crate::dto::ConnectivityScopeDto::Credential => {
            request.provider_id.is_none()
                && request.credential_id.is_some()
                && request.proxy_url.is_none()
        }
    };
    if valid {
        Ok(())
    } else {
        Err(AdminError::BadRequest(
            "connectivity scope does not match its ids".into(),
        ))
    }
}
