use gproxy_channel_api::{
    BoxFuture, ChannelError, ChannelLogin, CredentialAcquisition, DeviceInit, DevicePoll,
    DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::KimiChannel;

const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const OAUTH_HOST: &str = "https://auth.kimi.com";
const CODE_BASE: &str = "https://api.kimi.com/coding/v1";

impl ChannelLogin for KimiChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let host = crate::shared::login::field(ctx.provider_settings, "oauth_host")
                .unwrap_or(OAUTH_HOST)
                .trim_end_matches('/');
            let base_url =
                crate::shared::login::field(ctx.provider_settings, "base_url").unwrap_or(CODE_BASE);
            let device_id = device_id()?;
            let mut request = crate::shared::login::form_request(
                http::Method::POST,
                &format!("{host}/api/oauth/device_authorization"),
                &[("client_id", CLIENT_ID)],
            )?;
            super::identity::apply(request.headers_mut(), &device_id)?;
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "Kimi device code").await?;
            let state = serde_json::to_string(&PendingDevice {
                device_code: started.device_code,
                device_id,
                oauth_host: host.into(),
                base_url: base_url.into(),
            })
            .map_err(|error| ChannelError::Login(error.to_string()))?;
            Ok(DeviceInit {
                device_code: state,
                user_code: started.user_code,
                verification_uri: started.verification_uri_complete,
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
            let state: PendingDevice = serde_json::from_str(ctx.device_code)
                .map_err(|_| ChannelError::Login("invalid Kimi device state".into()))?;
            let mut request = crate::shared::login::form_request(
                http::Method::POST,
                &format!("{}/api/oauth/token", state.oauth_host.trim_end_matches('/')),
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("client_id", CLIENT_ID),
                    ("device_code", &state.device_code),
                ],
            )?;
            super::identity::apply(request.headers_mut(), &state.device_id)?;
            let reply: TokenReply =
                crate::shared::login::send_json_any_status(http, request, "Kimi device token")
                    .await?;
            if let Some(access_token) = reply.access_token.filter(|token| !token.is_empty()) {
                let refresh_token = reply
                    .refresh_token
                    .filter(|token| !token.is_empty())
                    .ok_or_else(|| {
                        ChannelError::Login("Kimi device token missing refresh_token".into())
                    })?;
                let secret = json!({
                    "auth_kind":"oauth",
                    "access_token":access_token,
                    "refresh_token":refresh_token,
                    "expires_at_ms":crate::shared::login::now_ms().saturating_add(
                        i64::try_from(reply.expires_in.unwrap_or(3_600)).unwrap_or(i64::MAX)
                            .saturating_mul(1_000),
                    ),
                    "device_id":state.device_id,
                    "base_url":state.base_url,
                    "oauth_host":state.oauth_host,
                });
                return Ok(DevicePoll::Ready(CredentialAcquisition::oauth(secret)));
            }
            match reply.error.as_deref() {
                Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
                Some(error) => Err(ChannelError::Login(format!(
                    "Kimi device token error: {error}"
                ))),
                None => Err(ChannelError::Login(
                    "Kimi device token returned neither access_token nor error".into(),
                )),
            }
        })
    }
}

fn device_id() -> Result<String, ChannelError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| ChannelError::Login("Kimi device id randomness failed".into()))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes(bytes[0..4].try_into().expect("four bytes")),
        u16::from_be_bytes(bytes[4..6].try_into().expect("two bytes")),
        u16::from_be_bytes(bytes[6..8].try_into().expect("two bytes")),
        u16::from_be_bytes(bytes[8..10].try_into().expect("two bytes")),
        u64::from_be_bytes([
            0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
        ])
    ))
}

#[derive(Serialize, Deserialize)]
struct PendingDevice {
    device_code: String,
    device_id: String,
    oauth_host: String,
    base_url: String,
}

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri_complete: String,
    #[serde(default = "default_interval")]
    interval: u64,
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
}

const fn default_interval() -> u64 {
    5
}
