use bytes::Bytes;
use gproxy_admin::dto::{OAuthConsentDto, OAuthDeviceDecision};
use gproxy_channel_api::{GPROXY_OAUTH_SCOPE, OAuthDevicePoll, OAuthError, OAuthService};
use http::{Response, StatusCode, request::Parts};
use serde::Deserialize;
use serde_json::{Value, json};

use super::wire;
use crate::host::{
    AppHost,
    oauth::{DEVICE_SECONDS, digest, now, store},
};

#[derive(Deserialize)]
struct DeviceQuery {
    user_code: String,
}

pub(super) async fn cancel(
    host: &AppHost,
    parts: &Parts,
    body: &[u8],
) -> Result<Response<Bytes>, OAuthError> {
    let request: Value = wire::parse(parts, body)?;
    let client_id = wire::string(&request, "client_id")?;
    let code = wire::string(&request, "device_code")?;
    if let Some(record) = host
        .services
        .store
        .oauth_device_by_digest(&digest(code))
        .await
        .map_err(store)?
        && record.client_id == client_id
    {
        host.services
            .store
            .cancel_oauth_device(record.id, now())
            .await
            .map_err(store)?;
    }
    Ok(wire::json_response(StatusCode::OK, &json!({})))
}

pub(super) async fn start(
    host: &AppHost,
    parts: &Parts,
    body: &[u8],
) -> Result<Response<Bytes>, OAuthError> {
    let request: Value = wire::parse(parts, body)?;
    let client_id = wire::string(&request, "client_id")?;
    if request
        .get("scope")
        .is_some_and(|scope| scope.as_str() != Some(GPROXY_OAUTH_SCOPE))
    {
        return Err(OAuthError::InvalidRequest);
    }
    let issuer = wire::issuer(parts)?;
    let started = host.device_start(None, client_id, &issuer).await?;
    let uri = format!("{issuer}/portal?oauth_device=");
    Ok(wire::json_response(
        StatusCode::OK,
        &json!({
            "device_code":started.device_auth_id, "user_code":started.user_code,
            "verification_uri":uri, "verification_uri_complete":format!("{uri}{}", wire::encode(&started.user_code)),
            "expires_in":DEVICE_SECONDS, "interval":started.interval_secs,
        }),
    ))
}

pub(super) async fn details(host: &AppHost, parts: &Parts) -> Result<Response<Bytes>, OAuthError> {
    let user = wire::browser(host, parts, false).await?;
    let query: DeviceQuery = wire::query(parts)?;
    let record = pending(host, &query.user_code).await?;
    let client = host.client(&record.client_id).await?;
    Ok(wire::json_response(
        StatusCode::OK,
        &OAuthConsentDto {
            client_id: client.client_id,
            client_name: client.name,
            user_name: user.name,
            scope: GPROXY_OAUTH_SCOPE.into(),
            user_code: Some(record.user_code),
        },
    ))
}

pub(super) async fn decide(
    host: &AppHost,
    parts: &Parts,
    body: &[u8],
) -> Result<Response<Bytes>, OAuthError> {
    let user = wire::browser(host, parts, true).await?;
    let decision: OAuthDeviceDecision = wire::parse(parts, body)?;
    let record = pending(host, &decision.user_code).await?;
    host.client(&record.client_id).await?;
    if decision.approved {
        host.device_approve(&user, &record.user_code, &wire::issuer(parts)?)
            .await?;
    } else {
        host.services
            .store
            .deny_oauth_device(record.id, now())
            .await
            .map_err(store)?;
    }
    Ok(wire::json_response(
        StatusCode::OK,
        &json!({"approved":decision.approved}),
    ))
}

async fn pending(
    host: &AppHost,
    code: &str,
) -> Result<gproxy_store::records::OAuthDeviceRecord, OAuthError> {
    if code.len() > 32 {
        return Err(OAuthError::InvalidRequest);
    }
    host.services
        .store
        .oauth_device_by_code(&crate::host::oauth::device::normalize_code(code))
        .await
        .map_err(store)?
        .filter(|record| {
            record.expires_at > now() && record.approved_at.is_none() && record.denied_at.is_none()
        })
        .ok_or(OAuthError::InvalidRequest)
}

pub(super) async fn exchange(
    host: &AppHost,
    request: &Value,
    client_id: &str,
    issuer: &str,
) -> Result<Response<Bytes>, OAuthError> {
    let code = wire::string(request, "device_code")?;
    let record = host
        .services
        .store
        .oauth_device_by_digest(&digest(code))
        .await
        .map_err(store)?
        .filter(|record| record.client_id == client_id)
        .ok_or(OAuthError::InvalidGrant)?;
    if record.expires_at <= now() {
        return Ok(wire::error(StatusCode::BAD_REQUEST, "expired_token"));
    }
    match host.device_poll(code, &record.user_code).await? {
        OAuthDevicePoll::Pending => Ok(wire::error(
            StatusCode::BAD_REQUEST,
            "authorization_pending",
        )),
        OAuthDevicePoll::Denied => Ok(wire::error(StatusCode::BAD_REQUEST, "access_denied")),
        OAuthDevicePoll::Ready {
            authorization_code,
            code_verifier,
            ..
        } => {
            let authorization = host
                .services
                .store
                .oauth_code(&digest(&authorization_code))
                .await
                .map_err(store)?
                .ok_or(OAuthError::InvalidGrant)?;
            Ok(wire::tokens(
                host.exchange_code(
                    &authorization_code,
                    client_id,
                    &authorization.redirect_uri,
                    &code_verifier,
                    issuer,
                )
                .await?,
            ))
        }
    }
}
