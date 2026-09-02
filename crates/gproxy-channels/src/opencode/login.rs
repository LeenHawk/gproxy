use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::json;

use super::OpenCodeChannel;

const CLIENT_ID: &str = "opencode-cli";
const CONSOLE_URL: &str = "https://console.opencode.ai";

impl ChannelLogin for OpenCodeChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let base = console_base(ctx.provider_settings);
            let request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{base}/auth/device/code"),
                &json!({ "client_id": CLIENT_ID }),
            )?;
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "OpenCode device code").await?;
            let path = started
                .verification_uri_complete
                .or(started.verification_uri)
                .unwrap_or_else(|| "/device".into());
            let verification_uri = if path.starts_with("http") {
                path
            } else {
                format!("{base}{path}")
            };
            let state = serde_json::to_string(&(base, started.device_code))
                .map_err(|error| ChannelError::Login(error.to_string()))?;
            Ok(DeviceInit {
                device_code: state,
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
            let (base, device_code): (String, String) = serde_json::from_str(ctx.device_code)
                .map_err(|_| ChannelError::Login("invalid OpenCode device state".into()))?;
            let request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{base}/auth/device/token"),
                &json!({
                    "grant_type":"urn:ietf:params:oauth:grant-type:device_code",
                    "device_code":device_code,
                    "client_id":CLIENT_ID,
                }),
            )?;
            let reply: TokenReply =
                crate::shared::login::send_json_any_status(http, request, "OpenCode device token")
                    .await?;
            if let Some(access_token) = reply.access_token.filter(|token| !token.is_empty()) {
                let mut secret = json!({
                    "api_key":access_token,
                    "access_token":access_token,
                    "refresh_token":reply.refresh_token.unwrap_or_default(),
                    "console_base_url":base,
                });
                if let Some(seconds) = reply.expires_in {
                    secret["expires_at_ms"] = json!(
                        crate::shared::login::now_ms()
                            .saturating_add(seconds.max(0).saturating_mul(1_000))
                    );
                } else {
                    secret["expiry_unknown"] = json!(true);
                }
                return Ok(DevicePoll::Ready(CredentialAcquisition::api_key(secret)));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
                Some(error) => Err(ChannelError::Login(format!(
                    "OpenCode device token error: {error}"
                ))),
                None => Err(ChannelError::Login(
                    "OpenCode device token returned neither access_token nor error".into(),
                )),
            }
        })
    }
}

fn console_base(settings: &serde_json::Value) -> String {
    crate::shared::login::field(settings, "console_base_url")
        .unwrap_or(CONSOLE_URL)
        .trim_end_matches('/')
        .into()
}

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri_complete: Option<String>,
    verification_uri: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
}

const fn default_interval() -> u64 {
    5
}
