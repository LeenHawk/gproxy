use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;

use super::CopilotCliChannel;

const CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
const START_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

impl ChannelLogin for CopilotCliChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        _ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let request = crate::shared::login::form_request(
                http::Method::POST,
                START_URL,
                &[("client_id", CLIENT_ID), ("scope", "read:user")],
            )?;
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "GitHub device code").await?;
            Ok(DeviceInit {
                device_code: started.device_code,
                user_code: started.user_code,
                verification_uri: started.verification_uri,
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
                    ("client_id", CLIENT_ID),
                    ("device_code", ctx.device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ],
            )?;
            let reply: DeviceToken =
                crate::shared::login::send_json_any_status(http, request, "GitHub device token")
                    .await?;
            if let Some(token) = reply.access_token.filter(|token| !token.is_empty()) {
                return Ok(DevicePoll::Ready(CredentialAcquisition::api_key(
                    serde_json::json!({ "github_token": token }),
                )));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
                Some(error) => Err(ChannelError::Login(format!(
                    "GitHub device token error: {error}"
                ))),
                None => Err(ChannelError::Login(
                    "GitHub device token returned neither access_token nor error".into(),
                )),
            }
        })
    }
}

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct DeviceToken {
    access_token: Option<String>,
    error: Option<String>,
}

const fn default_interval() -> u64 {
    5
}
