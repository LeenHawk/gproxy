use bytes::Bytes;
use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, BoxFuture, ChannelError, ChannelLogin,
    CredentialAcquisition, DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{CodexChannel, auth, profile};

const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const DEFAULT_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const SCOPE: &str = "openid profile email offline_access api.connectors.read api.connectors.invoke";
const DEVICE_START_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const DEVICE_POLL_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
const DEVICE_VERIFY_URI: &str = "https://auth.openai.com/codex/device";
const DEVICE_REDIRECT_URI: &str = "https://auth.openai.com/deviceauth/callback";

impl ChannelLogin for CodexChannel {
    fn authcode_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        let redirect = if ctx.redirect_uri.trim().is_empty() {
            DEFAULT_REDIRECT_URI
        } else {
            ctx.redirect_uri
        };
        let query = crate::shared::http::form(&[
            ("response_type", "code"),
            ("client_id", auth::CLIENT_ID),
            ("redirect_uri", redirect),
            ("scope", SCOPE),
            ("code_challenge", ctx.pkce_challenge),
            ("code_challenge_method", "S256"),
            ("id_token_add_organizations", "true"),
            ("codex_cli_simplified_flow", "true"),
            ("state", ctx.state),
            ("originator", auth::ORIGINATOR),
        ]);
        let started = AuthCodeStart {
            authorize_url: format!("{AUTHORIZE_URL}?{query}"),
            redirect_uri: redirect.into(),
            extra: None,
        };
        Box::pin(async move { Ok(Some(started)) })
    }

    fn authcode_exchange<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: AuthCodeExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        exchange(http, ctx.code, ctx.verifier, ctx.redirect_uri)
    }

    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        _ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let response = send_json(
                http,
                DEVICE_START_URL,
                &json!({ "client_id": auth::CLIENT_ID }),
            )
            .await?;
            if !response.status().is_success() {
                return Err(ChannelError::Login(format!(
                    "device start returned {}",
                    response.status()
                )));
            }
            let started: DeviceStartResponse = serde_json::from_slice(response.body())
                .map_err(|_| ChannelError::Login("invalid device start response".into()))?;
            let device_code = serde_json::to_string(&DeviceState {
                device_auth_id: started.device_auth_id,
                user_code: started.user_code.clone(),
            })
            .map_err(|_| ChannelError::Login("invalid device state".into()))?;
            Ok(DeviceInit {
                device_code,
                user_code: started.user_code,
                verification_uri: DEVICE_VERIFY_URI.into(),
                interval_secs: started.interval.max(1),
            })
        })
    }

    fn device_poll<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DevicePollCtx<'a>,
    ) -> BoxFuture<'a, Result<DevicePoll, ChannelError>> {
        Box::pin(async move {
            let state: DeviceState = serde_json::from_str(ctx.device_code)
                .map_err(|_| ChannelError::Login("invalid device state".into()))?;
            let response = send_json(
                http,
                DEVICE_POLL_URL,
                &json!({
                    "device_auth_id": state.device_auth_id,
                    "user_code": state.user_code,
                }),
            )
            .await?;
            match response.status().as_u16() {
                403 | 404 => Ok(DevicePoll::Pending),
                200..=299 => {
                    let ready: DeviceReadyResponse = serde_json::from_slice(response.body())
                        .map_err(|_| ChannelError::Login("invalid device poll response".into()))?;
                    exchange(
                        http,
                        &ready.authorization_code,
                        &ready.code_verifier,
                        DEVICE_REDIRECT_URI,
                    )
                    .await
                    .map(DevicePoll::Ready)
                }
                _ => Ok(DevicePoll::Denied),
            }
        })
    }
}

fn exchange<'a>(
    http: &'a dyn SimpleHttp,
    code: &'a str,
    verifier: &'a str,
    redirect: &'a str,
) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
    let body = crate::shared::http::form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", auth::CLIENT_ID),
        ("code_verifier", verifier),
    ]);
    let request = http::Request::post(auth::TOKEN_URL)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(body));
    let mut request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(ChannelError::Login(error.to_string())) }),
    };
    request
        .extensions_mut()
        .insert(profile::CLIENT_PROFILE.clone());
    Box::pin(async move {
        let response = http.send(request).await?;
        if !response.status().is_success() {
            return Err(ChannelError::Login(format!(
                "token endpoint returned {}",
                response.status()
            )));
        }
        let token = serde_json::from_slice(response.body())
            .map_err(|_| ChannelError::Login("invalid token response".into()))?;
        auth::login_secret(&token).map(CredentialAcquisition::oauth)
    })
}

async fn send_json(
    http: &dyn SimpleHttp,
    uri: &str,
    body: &Value,
) -> Result<http::Response<Bytes>, ChannelError> {
    let payload = serde_json::to_vec(body)
        .map_err(|_| ChannelError::Login("invalid login request".into()))?;
    let request = http::Request::post(uri)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(payload))
        .map_err(|error| ChannelError::Login(error.to_string()))?;
    http.send(request).await
}

#[derive(Deserialize)]
struct DeviceStartResponse {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default = "default_interval")]
    interval: u64,
}
#[derive(serde::Serialize, Deserialize)]
struct DeviceState {
    device_auth_id: String,
    user_code: String,
}
#[derive(Deserialize)]
struct DeviceReadyResponse {
    authorization_code: String,
    code_verifier: String,
}
fn default_interval() -> u64 {
    5
}
