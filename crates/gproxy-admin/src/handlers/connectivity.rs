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
