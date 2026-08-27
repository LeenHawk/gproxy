pub(crate) mod state;

use bytes::Bytes;
use gproxy_channel_api::DevicePoll;
use gproxy_store::records::CredentialInput;
use http::{Response, StatusCode};
use serde_json::{Value, json};

use crate::dto::{
    AuthCodeCompleteRequest, AuthCodeStartRequest, AuthCodeStartResponse, CookieExchangeRequest,
    DevicePollRequest, DevicePollResponse, DeviceStartRequest, DeviceStartResponse, IdResponse,
};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(crate) async fn authcode_start(
    app: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: AuthCodeStartRequest = util::parse(body)?;
    let channel = provider_channel(app, request.provider_id).await?;
    let (verifier, challenge) = state::pkce()?;
    let flow_state = state::session_id()?;
    let params = request.params.unwrap_or_else(|| json!({}));
    let started = app
        .login_authcode_start(
            &channel,
            request.provider_id,
            &params,
            request.redirect_uri.as_deref().unwrap_or_default(),
            &flow_state,
            &challenge,
        )
        .await?
        .ok_or_else(|| AdminError::BadRequest("channel has no authcode login".into()))?;
    let id = state::session_id()?;
    state::store_authcode(
        app,
        &id,
        &state::AuthCodeSession {
            channel,
            provider_id: request.provider_id,
            verifier,
            flow_state,
            redirect_uri: started.redirect_uri,
            extra: started.extra,
        },
    )
    .await?;
    response::json(
        StatusCode::OK,
        &AuthCodeStartResponse {
            login_session_id: id,
            authorize_url: started.authorize_url,
        },
    )
}

pub(crate) async fn authcode_complete(
    app: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: AuthCodeCompleteRequest = util::parse(body)?;
    let session = state::authcode(app, &request.login_session_id).await?;
    let (code, callback_state) = callback(&request.callback_url)?;
    if callback_state != session.flow_state {
        return Err(AdminError::BadRequest(
            "login callback state mismatch".into(),
        ));
    }
    let secret = app
        .login_authcode_exchange(
            &session.channel,
            session.provider_id,
            &code,
            &session.verifier,
            &session.redirect_uri,
            session.extra.as_ref(),
        )
        .await?;
    state::delete(app, &request.login_session_id).await?;
    created(app, session.provider_id, request.label, "oauth", &secret).await
}

pub(crate) async fn device_start(
    app: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: DeviceStartRequest = util::parse(body)?;
    let channel = provider_channel(app, request.provider_id).await?;
    let params = request.params.unwrap_or_else(|| json!({}));
    let started = app
        .login_device_start(&channel, request.provider_id, &params)
        .await?;
    let id = state::session_id()?;
    state::store_device(
        app,
        &id,
        &state::DeviceSession {
            channel,
            provider_id: request.provider_id,
            label: request.label,
            device_code: started.device_code,
        },
    )
    .await?;
    response::json(
        StatusCode::OK,
        &DeviceStartResponse {
            login_session_id: id,
            user_code: started.user_code,
            verification_uri: started.verification_uri,
            interval_secs: started.interval_secs,
        },
    )
}

pub(crate) async fn device_poll(
    app: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: DevicePollRequest = util::parse(body)?;
    let session = state::device(app, &request.login_session_id).await?;
    match app
        .login_device_poll(&session.channel, session.provider_id, &session.device_code)
        .await?
    {
        DevicePoll::Pending => response::json(StatusCode::OK, &DevicePollResponse::Pending),
        DevicePoll::Denied => {
            state::delete(app, &request.login_session_id).await?;
            response::json(StatusCode::OK, &DevicePollResponse::Denied)
        }
        DevicePoll::Ready(secret) => {
            state::delete(app, &request.login_session_id).await?;
            let credential =
                insert(app, session.provider_id, session.label, "oauth", &secret).await?;
            response::json(StatusCode::OK, &DevicePollResponse::Ready { credential })
        }
    }
}

pub(crate) async fn cookie_exchange(
    app: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: CookieExchangeRequest = util::parse(body)?;
    let channel = provider_channel(app, request.provider_id).await?;
    let secret = app
        .login_cookie_exchange(&channel, request.provider_id, &request.cookie)
        .await?;
    created(app, request.provider_id, request.label, "cookie", &secret).await
}

async fn created(
    app: &impl State,
    provider_id: i64,
    label: Option<String>,
    kind: &str,
    secret: &Value,
) -> Result<Response<Bytes>, AdminError> {
    let credential = insert(app, provider_id, label, kind, secret).await?;
    response::json(StatusCode::CREATED, &credential)
}

async fn insert(
    app: &impl State,
    provider_id: i64,
    label: Option<String>,
    kind: &str,
    secret: &Value,
) -> Result<IdResponse, AdminError> {
    let id = app
        .store()
        .insert_credential(&CredentialInput {
            provider_id,
            label,
            kind: kind.into(),
            envelope: app.seal_credential(secret)?,
            enabled: true,
            weight: 100,
            rpm_limit: None,
            tpm_limit: None,
            proxy_url: None,
            tls_fingerprint: None,
        })
        .await?;
    app.reload().await?;
    Ok(IdResponse { id })
}

async fn provider_channel(app: &impl State, provider_id: i64) -> Result<String, AdminError> {
    app.store()
        .control_snapshot()
        .await?
        .providers
        .into_iter()
        .find(|provider| provider.id == provider_id && provider.enabled)
        .map(|provider| provider.channel)
        .ok_or(AdminError::NotFound)
}

fn callback(url: &str) -> Result<(String, String), AdminError> {
    let uri = url
        .parse::<http::Uri>()
        .map_err(|_| AdminError::BadRequest("invalid login callback".into()))?;
    let fields = form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
        .into_owned()
        .collect::<std::collections::BTreeMap<_, _>>();
    let required = |name: &str| {
        fields
            .get(name)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .ok_or_else(|| AdminError::BadRequest("invalid login callback".into()))
    };
    Ok((required("code")?, required("state")?))
}
