//! Cline account login: WorkOS device flow, then registration with Cline.
//!
//! Cline delegates identity to WorkOS but does not use WorkOS tokens directly.
//! The device flow mints a WorkOS token pair, `POST /api/v1/auth/register`
//! trades that pair for Cline's own tokens, and only the Cline tokens
//! authenticate inference. `POST /api/v1/auth/refresh` rotates them afterwards.
//!
//! Every Cline API reply is wrapped in `{"success": bool, "data": …}`; see
//! [`unwrap_envelope`].

use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{Value, json};

use super::auth;
use crate::channel::{ChannelError, DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

/// Cline's public WorkOS client (production environment).
const WORKOS_CLIENT_ID: &str = "client_01K3A541FN8TA3EPPHTD2325AR";
const WORKOS_DEVICE_AUTH_URL: &str = "https://api.workos.com/user_management/authorize/device";
const WORKOS_AUTHENTICATE_URL: &str = "https://api.workos.com/user_management/authenticate";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    #[serde(default = "default_interval")]
    interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// One WorkOS `/authenticate` reply: the token pair, or an OAuth `error` code.
#[derive(Deserialize)]
struct WorkOsTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
}

/// `data` of a Cline `/auth/register` or `/auth/refresh` reply.
#[derive(Deserialize)]
struct ClineTokens {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: Option<String>,
    #[serde(rename = "userInfo")]
    user_info: Option<UserInfo>,
}

#[derive(Deserialize)]
struct UserInfo {
    #[serde(rename = "clineUserId")]
    cline_user_id: Option<String>,
    email: Option<String>,
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
) -> Result<DeviceInit, ChannelError> {
    let form = format!("client_id={WORKOS_CLIENT_ID}");
    let parsed: DeviceAuthorization = send_json(
        client,
        form_post(WORKOS_DEVICE_AUTH_URL, form)?,
        "device authorization",
    )
    .await?;
    Ok(DeviceInit {
        device_code: parsed.device_code,
        user_code: parsed.user_code,
        verification_url: parsed
            .verification_uri_complete
            .unwrap_or(parsed.verification_uri),
        interval_secs: parsed.interval,
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    device_code: &str,
) -> Result<DevicePoll, ChannelError> {
    let form =
        format!("grant_type={DEVICE_GRANT}&device_code={device_code}&client_id={WORKOS_CLIENT_ID}");
    // WorkOS answers non-2xx while the operator has not authorized yet, so the
    // JSON `error` field — not the status — drives the decision.
    let tokens: WorkOsTokens = send_json_any_status(
        client,
        form_post(WORKOS_AUTHENTICATE_URL, form)?,
        "device token",
    )
    .await?;

    let (Some(access), Some(refresh)) = (
        tokens.access_token.filter(|t| !t.is_empty()),
        tokens.refresh_token.filter(|t| !t.is_empty()),
    ) else {
        return match tokens.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
            Some("access_denied") | Some("expired_token") | Some("invalid_grant") => {
                Ok(DevicePoll::Denied)
            }
            // An unknown error is terminal — surface it rather than poll forever.
            Some(other) => Err(ChannelError::Build(format!("device poll error: {other}"))),
            None => Err(ChannelError::Build(
                "device poll: WorkOS returned neither tokens nor an error".into(),
            )),
        };
    };

    // WorkOS proves identity; Cline issues the tokens that authorize inference.
    let base = super::base_url(settings);
    let registered: ClineTokens = post_json(
        client,
        &format!("{base}/auth/register"),
        &json!({ "accessToken": access, "refreshToken": refresh }),
        "auth register",
    )
    .await?;
    Ok(DevicePoll::Ready(secret_from(registered, &Value::Null)))
}

pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    settings: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = auth::field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    let base = super::base_url(settings);
    let refreshed: ClineTokens = post_json(
        client,
        &format!("{base}/auth/refresh"),
        &json!({ "refreshToken": refresh_token, "grantType": "refresh_token" }),
        "auth refresh",
    )
    .await?;
    Ok(secret_from(refreshed, secret))
}

/// Whether the Cline access token is close enough to expiry to rotate. A
/// credential holding only a pasted `api_key` never refreshes.
pub(super) fn needs_refresh(secret: &Value) -> bool {
    if auth::field(secret, "refresh_token").is_none() {
        return false;
    }
    let Some(token) = auth::field(secret, "access_token") else {
        // Registered but token-less: refresh is the only way forward.
        return true;
    };
    // An undecodable token cannot be trusted to still be valid.
    let Some(exp) = auth::token_expiry_secs(token) else {
        return true;
    };
    crate::util::time::unix_now() > exp - EXPIRY_SKEW_SECS
}

/// Rotate slightly before expiry so a refresh never races a live request.
const EXPIRY_SKEW_SECS: i64 = 300;

/// Merge a token reply over the previous secret, preserving fields the reply
/// omits — a refresh may reuse the existing refresh token or skip `userInfo`.
fn secret_from(tokens: ClineTokens, previous: &Value) -> Value {
    let mut out = previous.clone();
    if !out.is_object() {
        out = json!({});
    }
    let obj = out.as_object_mut().expect("object");
    obj.insert("api_key".into(), Value::String(tokens.access_token.clone()));
    obj.insert("access_token".into(), Value::String(tokens.access_token));
    if let Some(token) = tokens.refresh_token.filter(|t| !t.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(token));
    }
    if let Some(info) = tokens.user_info {
        if let Some(id) = info.cline_user_id.filter(|id| !id.is_empty()) {
            obj.insert("user_id".into(), Value::String(id));
        }
        if let Some(email) = info.email.filter(|email| !email.is_empty()) {
            obj.insert("email".into(), Value::String(email));
        }
    }
    out
}

fn form_post(url: &str, form: String) -> Result<Request<Bytes>, ChannelError> {
    Request::post(url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(form))
        .map_err(|e| ChannelError::Build(format!("login request build: {e}")))
}

async fn post_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    body: &Value,
    what: &str,
) -> Result<T, ChannelError> {
    let req = Request::post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body.to_string()))
        .map_err(|e| ChannelError::Build(format!("{what} request build: {e}")))?;
    let value: Value = send_json(client, req, what).await?;
    serde_json::from_value(unwrap_envelope(value, what)?)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

/// Cline wraps replies in `{"success": bool, "data": …}`. A `success: false`
/// body carries the operator-facing reason, so surface it verbatim.
pub(super) fn unwrap_envelope(value: Value, what: &str) -> Result<Value, ChannelError> {
    let Some(success) = value.get("success").and_then(Value::as_bool) else {
        return Ok(value);
    };
    if !success {
        let reason = value
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("request failed");
        return Err(ChannelError::Build(format!("{what}: {reason}")));
    }
    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

async fn send_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    req: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}

/// Parse the body whatever the status: a pending device poll is a non-2xx
/// `{"error": …}` payload.
async fn send_json_any_status<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    req: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
    serde_json::from_slice(&resp.into_body())
        .map_err(|e| ChannelError::Build(format!("{what} response parse: {e}")))
}
