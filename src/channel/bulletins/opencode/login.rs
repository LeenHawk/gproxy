//! OpenCode Console device-code login and token refresh.
//!
//! The console account token is NOT itself a gateway credential: Zen/Go
//! authenticate strictly against stored API keys (the gateway looks the bearer
//! up in its key table), so a console access token presented to `/zen/v1/*` is
//! rejected. What the account DOES give is the workspace's managed config — the
//! same `GET {server}/api/config` document the OpenCode CLI merges — whose
//! `provider.<tier>.options.apiKey` is a real gateway key.
//!
//! So the flow is: device-code login → console tokens → pull the managed config
//! → keep its `apiKey` as the credential, retaining the tokens only to re-pull
//! it later. An account with no managed config fails the login loudly instead of
//! persisting a credential that would 401 on every request.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use super::console;
use crate::channel::{ChannelError, DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

const CLIENT_ID: &str = "opencode-cli";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Deserialize)]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri_complete: Option<String>,
    verification_uri: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// One `/auth/device/token` reply: tokens on success, an OAuth `error` code
/// while the operator has not finished authorizing.
#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
) -> Result<DeviceInit, ChannelError> {
    let base = console::base_url(settings, &Value::Null);
    let req = console::post(
        &format!("{base}/auth/device/code"),
        &json!({ "client_id": CLIENT_ID }),
    )?;
    let parsed: DeviceCode = console::send_json(client, req, "device code").await?;
    // Both verification URLs come back console-relative (`/device?user_code=…`).
    let path = parsed
        .verification_uri_complete
        .or(parsed.verification_uri)
        .unwrap_or_else(|| "/device".to_string());
    let verification_url = if path.starts_with("http") {
        path
    } else {
        format!("{base}{path}")
    };
    Ok(DeviceInit {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_url,
        interval_secs: parsed.interval,
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    device_code: &str,
    tier: &str,
) -> Result<DevicePoll, ChannelError> {
    let base = console::base_url(settings, &Value::Null);
    let req = console::post(
        &format!("{base}/auth/device/token"),
        &json!({
            "grant_type": DEVICE_GRANT,
            "device_code": device_code,
            "client_id": CLIENT_ID,
        }),
    )?;
    // The console answers non-2xx while a poll is pending, so the JSON `error`
    // field — not the status — drives the decision.
    let reply: TokenReply = console::send_json_any_status(client, req, "device token").await?;

    let Some(access) = reply.access_token.filter(|token| !token.is_empty()) else {
        return match reply.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
            Some("access_denied") | Some("expired_token") => Ok(DevicePoll::Denied),
            // An unknown error is terminal — surface it rather than poll forever.
            Some(other) => Err(ChannelError::Build(format!("device poll error: {other}"))),
            None => Err(ChannelError::Build(
                "device poll: neither access_token nor error".into(),
            )),
        };
    };

    let org = console::first_org(client, base, &access).await?;
    let (org_id, org_name) = org.map_or((None, None), |org| (Some(org.id), Some(org.name)));
    let api_key = console::workspace_key(client, base, &access, org_id.as_deref(), tier)
        .await?
        .ok_or_else(|| {
            ChannelError::InvalidCredential(format!(
                "this OpenCode Console account publishes no managed `{tier}` API key. \
                 Console sign-in only distributes workspace-managed keys; copy the key \
                 from the OpenCode dashboard and add it as `api_key` instead"
            ))
        })?;

    Ok(DevicePoll::Ready(json!({
        "api_key": api_key,
        "access_token": access,
        "refresh_token": reply.refresh_token.unwrap_or_default(),
        "expires_at_ms": expires_at_ms(reply.expires_in),
        "console_base_url": base,
        "org_id": org_id,
        "org_name": org_name,
    })))
}

/// Rotate the console tokens and re-pull the managed key. A failed config pull
/// keeps the stored key: a transient console outage must not break an otherwise
/// working credential.
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    settings: &Value,
    tier: &str,
) -> Result<Value, ChannelError> {
    let base = console::base_url(settings, secret).to_string();
    let refresh_token = secret
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    let req = console::post(
        &format!("{base}/auth/device/token"),
        &json!({
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
            "client_id": CLIENT_ID,
        }),
    )?;
    let reply: TokenReply = console::send_json(client, req, "token refresh").await?;
    let access = reply
        .access_token
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            ChannelError::InvalidCredential("refresh returned no access_token".into())
        })?;

    let org_id = secret
        .get("org_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let fresh_key = console::workspace_key(client, &base, &access, org_id.as_deref(), tier)
        .await
        .ok()
        .flatten();

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(access));
    if let Some(token) = reply.refresh_token.filter(|token| !token.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(token));
    }
    obj.insert(
        "expires_at_ms".into(),
        json!(expires_at_ms(reply.expires_in)),
    );
    if let Some(key) = fresh_key {
        obj.insert("api_key".into(), Value::String(key));
    }
    Ok(out)
}

/// Whether the console tokens are close enough to expiry to re-exchange. A
/// credential holding only a pasted `api_key` never refreshes.
pub(super) fn needs_refresh(secret: &Value) -> bool {
    let has_refresh_token = secret
        .get("refresh_token")
        .and_then(Value::as_str)
        .is_some_and(|token| !token.trim().is_empty());
    if !has_refresh_token {
        return false;
    }
    let expires_at_ms = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms > expires_at_ms - EXPIRY_SKEW_MS
}

/// Re-exchange slightly before expiry so a refresh never races a live request.
const EXPIRY_SKEW_MS: i64 = 60_000;

fn expires_at_ms(expires_in: Option<i64>) -> i64 {
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms.saturating_add(expires_in.unwrap_or(0).saturating_mul(1000))
}
