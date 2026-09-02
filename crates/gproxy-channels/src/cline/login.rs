use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::ClineChannel;

const CLIENT_ID: &str = "client_01K3A541FN8TA3EPPHTD2325AR";
const START_URL: &str = "https://api.workos.com/user_management/authorize/device";
const TOKEN_URL: &str = "https://api.workos.com/user_management/authenticate";

impl ChannelLogin for ClineChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        _ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let request = crate::shared::login::form_request(
                http::Method::POST,
                START_URL,
                &[("client_id", CLIENT_ID)],
            )?;
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "WorkOS device code").await?;
            Ok(DeviceInit {
                device_code: started.device_code,
                user_code: started.user_code,
                verification_uri: started
                    .verification_uri_complete
                    .unwrap_or(started.verification_uri),
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
            let request = crate::shared::login::form_request(
                http::Method::POST,
                TOKEN_URL,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", ctx.device_code),
                    ("client_id", CLIENT_ID),
                ],
            )?;
            let workos: WorkOsTokens =
                crate::shared::login::send_json_any_status(http, request, "WorkOS device token")
                    .await?;
            let (Some(access_token), Some(refresh_token)) = (
                workos.access_token.filter(|token| !token.is_empty()),
                workos.refresh_token.filter(|token| !token.is_empty()),
            ) else {
                return match workos.error.as_deref() {
                    Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                    Some("access_denied") | Some("expired_token") | Some("invalid_grant") => {
                        Ok(DevicePoll::Denied)
                    }
                    Some(error) => Err(ChannelError::Login(format!(
                        "WorkOS device token error: {error}"
                    ))),
                    None => Err(ChannelError::Login(
                        "WorkOS device token returned neither tokens nor error".into(),
                    )),
                };
            };
            let base = super::prepare::base_url(ctx.provider_settings);
            let request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{base}/auth/register"),
                &json!({ "accessToken":access_token, "refreshToken":refresh_token }),
            )?;
            let envelope: Value =
                crate::shared::login::send_json(http, request, "Cline auth register").await?;
            Ok(DevicePoll::Ready(CredentialAcquisition::api_key(secret(
                unwrap_envelope(envelope)?,
            )?)))
        })
    }
}

fn unwrap_envelope(value: Value) -> Result<Value, ChannelError> {
    if value.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(ChannelError::Login(
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Cline registration failed")
                .into(),
        ));
    }
    Ok(value.get("data").cloned().unwrap_or(value))
}

fn secret(value: Value) -> Result<Value, ChannelError> {
    let access = crate::shared::login::field(&value, "accessToken")
        .ok_or_else(|| ChannelError::Login("Cline registration missing accessToken".into()))?;
    let mut secret = json!({ "api_key":access, "access_token":access });
    if let Some(refresh) = crate::shared::login::field(&value, "refreshToken") {
        secret["refresh_token"] = Value::String(refresh.into());
    }
    if let Some(info) = value.get("userInfo") {
        for (source, target) in [("clineUserId", "user_id"), ("email", "email")] {
            if let Some(value) = crate::shared::login::field(info, source) {
                secret[target] = Value::String(value.into());
            }
        }
    }
    Ok(secret)
}

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct WorkOsTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
}

const fn default_interval() -> u64 {
    5
}
