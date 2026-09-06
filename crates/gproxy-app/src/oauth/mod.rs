mod authorize;
mod device;
mod wire;

use bytes::Bytes;
use gproxy_channel_api::{OAuthError, OAuthService};
use http::{Method, Response, StatusCode, request::Parts};
use serde_json::{Value, json};

use crate::host::AppHost;
use wire::{error, issuer, json_response, parse, string, tokens};

pub(crate) async fn dispatch(
    host: &AppHost,
    parts: &Parts,
    body: Bytes,
) -> Option<Response<Bytes>> {
    if !parts.uri.path().starts_with("/oauth/") {
        return None;
    }
    if body.len() > 16 * 1024 {
        return Some(error(StatusCode::PAYLOAD_TOO_LARGE, "invalid_request"));
    }
    let result = match (&parts.method, parts.uri.path()) {
        (&Method::GET, "/oauth/authorize") => authorize::start(host, parts).await,
        (&Method::GET, "/oauth/authorize/details") => authorize::details(host, parts).await,
        (&Method::POST, "/oauth/authorize") => authorize::decide(host, parts, &body).await,
        (&Method::POST, "/oauth/token") => token(host, parts, &body).await,
        (&Method::POST, "/oauth/revoke") => revoke(host, parts, &body).await,
        (&Method::POST, "/oauth/device/code") => device::start(host, parts, &body).await,
        (&Method::POST, "/oauth/device/cancel") => device::cancel(host, parts, &body).await,
        (&Method::GET, "/oauth/device/details") => device::details(host, parts).await,
        (&Method::POST, "/oauth/device/decision") => device::decide(host, parts, &body).await,
        _ => return Some(error(StatusCode::NOT_FOUND, "not_found")),
    };
    Some(match result {
        Ok(response) => response,
        Err(OAuthError::InvalidRequest) => error(StatusCode::BAD_REQUEST, "invalid_request"),
        Err(OAuthError::InvalidClient) => error(StatusCode::BAD_REQUEST, "invalid_client"),
        Err(OAuthError::InvalidGrant) => error(StatusCode::BAD_REQUEST, "invalid_grant"),
        Err(OAuthError::AccessDenied) => error(StatusCode::FORBIDDEN, "access_denied"),
        Err(OAuthError::TemporarilyUnavailable) => {
            error(StatusCode::SERVICE_UNAVAILABLE, "temporarily_unavailable")
        }
        Err(OAuthError::Store(message)) => {
            tracing::error!(error = %message, "OAuth persistence failed");
            error(StatusCode::INTERNAL_SERVER_ERROR, "server_error")
        }
    })
}

async fn token(host: &AppHost, parts: &Parts, body: &Bytes) -> Result<Response<Bytes>, OAuthError> {
    let request: Value = parse(parts, body)?;
    let client_id = string(&request, "client_id")?;
    host.client(client_id).await?;
    let issuer = issuer(parts)?;
    let issued = match string(&request, "grant_type")? {
        "authorization_code" => {
            host.exchange_code(
                string(&request, "code")?,
                client_id,
                string(&request, "redirect_uri")?,
                string(&request, "code_verifier")?,
                &issuer,
            )
            .await?
        }
        "refresh_token" => {
            host.refresh(string(&request, "refresh_token")?, client_id, &issuer)
                .await?
        }
        "urn:ietf:params:oauth:grant-type:device_code" => {
            return device::exchange(host, &request, client_id, &issuer).await;
        }
        _ => return Ok(error(StatusCode::BAD_REQUEST, "unsupported_grant_type")),
    };
    Ok(tokens(issued))
}

async fn revoke(
    host: &AppHost,
    parts: &Parts,
    body: &Bytes,
) -> Result<Response<Bytes>, OAuthError> {
    let request: Value = parse(parts, body)?;
    let client_id = string(&request, "client_id")?;
    host.client(client_id).await?;
    let token = string(&request, "token")?;
    if let Some(record) = host
        .services
        .store
        .oauth_token(&crate::host::oauth::digest(token))
        .await
        .map_err(crate::host::oauth::store)?
        && record.grant.client_id == client_id
    {
        host.revoke(token).await?;
    }
    Ok(json_response(StatusCode::OK, &json!({})))
}
