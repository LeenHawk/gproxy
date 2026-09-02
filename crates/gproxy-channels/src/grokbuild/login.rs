use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::GrokBuildChannel;

const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const START_URL: &str = "https://auth.x.ai/oauth2/device/code";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const SCOPE: &str = "openid profile email offline_access grok-cli:access api:access conversations:read conversations:write workspaces:read workspaces:write";

impl ChannelLogin for GrokBuildChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        _ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let request = crate::shared::login::form_request(
                http::Method::POST,
                START_URL,
                &[("client_id", CLIENT_ID), ("scope", SCOPE)],
            )?;
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "xAI device code").await?;
            let verification_uri = started
                .verification_uri_complete
                .or(started.verification_uri)
                .ok_or_else(|| {
                    ChannelError::Login("xAI device code missing verification_uri".into())
                })?;
            Ok(DeviceInit {
                device_code: started.device_code,
                user_code: started.user_code,
                verification_uri,
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
                    ("client_id", CLIENT_ID),
                    ("device_code", ctx.device_code),
                ],
            )?;
            let reply: DeviceToken =
                crate::shared::login::send_json_any_status(http, request, "xAI device token")
                    .await?;
            if let Some(access_token) = reply.access_token.filter(|token| !token.is_empty()) {
                let mut secret = json!({
                    "auth_kind":"oauth",
                    "type":"xai",
                    "access_token":access_token,
                    "expires_at_ms":crate::shared::login::now_ms().saturating_add(
                        i64::try_from(reply.expires_in.unwrap_or(3_600)).unwrap_or(i64::MAX)
                            .saturating_mul(1_000),
                    ),
                    "token_endpoint":TOKEN_URL,
                });
                copy(&mut secret, "refresh_token", reply.refresh_token);
                copy(&mut secret, "id_token", reply.id_token);
                return Ok(DevicePoll::Ready(CredentialAcquisition::oauth(secret)));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
                Some(error) => Err(ChannelError::Login(format!(
                    "xAI device token error: {error}"
                ))),
                None => Err(ChannelError::Login(
                    "xAI device token returned neither access_token nor error".into(),
                )),
            }
        })
    }
}

fn copy(secret: &mut Value, name: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        secret[name] = Value::String(value);
    }
}

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: Option<String>,
    verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct DeviceToken {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    id_token: Option<String>,
    error: Option<String>,
}

const fn default_interval() -> u64 {
    5
}
