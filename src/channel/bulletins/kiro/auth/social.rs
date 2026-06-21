//! Kiro social login — GitHub / Google device-code via the Kiro desktop auth
//! service (`*.auth.desktop.kiro.dev`).
//!
//! A CAPTURED device-code flow (`kiro-cli-chat`): [`device_start`] POSTs
//! `{"clientId":"Kiro-CLI","loginProvider":<GitHub|Google>}` to
//! `/oauth/device/authorization`, the operator authorizes in a browser, and
//! [`device_poll`] returns the plaintext secret once `authorized`. Device tokens
//! carry no expiry; [`refresh`] hits `{auth_base}/refreshToken` with
//! `{"refreshToken": rt}` (no client creds — the social path is unauthenticated
//! beyond the refresh token) and re-asserts `profileArn`.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::channel::ChannelError;
use crate::channel::login::{DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

use super::{json_post, now_ms, secret_str};

/// Default Kiro desktop auth host (device login + social refresh). Override via
/// `settings.auth_base_url`.
pub(super) const DEFAULT_AUTH_BASE_URL: &str = "https://prod.us-east-1.auth.desktop.kiro.dev";
/// Literal `clientId` the device flow sends (there is NO client_secret).
const DEVICE_CLIENT_ID: &str = "Kiro-CLI";
/// Default social login provider when `params` does not select one. The Kiro
/// desktop auth service accepts `Github` / `Google` (canonical Display form is
/// `GitHub`/`Google`, but the captured live request used `Github`, a documented
/// alias — kept verbatim for the GitHub path that is known-good).
const DEVICE_LOGIN_PROVIDER: &str = "Github";
/// Device tokens carry no expiry; assume a 1h access-token life so the
/// proactive refresh path has an `expires_at_ms` to compare against.
const DEVICE_TOKEN_TTL_MS: i64 = 3_600_000;

/// Map an operator `params.login_provider` (case-insensitive `github`/`google`,
/// or a bare provider string) to the wire `loginProvider` value. Defaults to
/// [`DEVICE_LOGIN_PROVIDER`] (GitHub) when absent/unrecognised.
fn login_provider(params: &Value) -> &'static str {
    match params
        .get("login_provider")
        .or_else(|| params.get("provider"))
        .and_then(Value::as_str)
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("google") => "Google",
        Some("github") => "Github",
        _ => DEVICE_LOGIN_PROVIDER,
    }
}

/// Build the device-authorization request body: the captured `kiro-cli-chat`
/// posts `{"clientId":"Kiro-CLI","loginProvider":"Github"}` (or `"Google"`).
fn device_start_body(provider: &str) -> Value {
    json!({
        "clientId": DEVICE_CLIENT_ID,
        "loginProvider": provider,
    })
}

/// `POST {auth_base}/oauth/device/authorization` response (camelCase).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceAuthorizationResponse {
    device_code: Option<String>,
    user_code: Option<String>,
    verification_uri_complete: Option<String>,
    verification_uri: Option<String>,
    interval_in_milliseconds: Option<u64>,
}

/// Begin a device-code login: `POST {auth_base}/oauth/device/authorization` with
/// `{"clientId":"Kiro-CLI","loginProvider":<provider>}` (provider from
/// `params.login_provider`, default GitHub). Maps the response to a
/// [`DeviceInit`] (verification URL prefers the `complete` form; interval is the
/// provider's milliseconds / 1000, floored to 1s, defaulting to 5s).
pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
) -> Result<DeviceInit, ChannelError> {
    let url = format!(
        "{}/oauth/device/authorization",
        DEFAULT_AUTH_BASE_URL.trim_end_matches('/')
    );
    let body = device_start_body(login_provider(params));
    let resp: DeviceAuthorizationResponse = json_post(client, &url, &body).await?;

    let device_code = resp
        .device_code
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ChannelError::Build("device authorization missing deviceCode".into()))?;
    let user_code = resp
        .user_code
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ChannelError::Build("device authorization missing userCode".into()))?;
    let verification_url = resp
        .verification_uri_complete
        .or(resp.verification_uri)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ChannelError::Build("device authorization missing verificationUri".into())
        })?;
    let interval_secs = resp
        .interval_in_milliseconds
        .map(|ms| (ms / 1000).max(1))
        .unwrap_or(5);

    Ok(DeviceInit {
        device_code,
        user_code,
        verification_url,
        interval_secs,
    })
}

/// `POST {auth_base}/oauth/device/poll` response (camelCase).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevicePollResponse {
    status: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    profile_arn: Option<String>,
    identity_provider: Option<String>,
}

/// Build the device-poll request body: `{"deviceCode":<code>,"clientId":"Kiro-CLI"}`.
fn device_poll_body(device_code: &str) -> Value {
    json!({
        "deviceCode": device_code,
        "clientId": DEVICE_CLIENT_ID,
    })
}

/// Poll a pending device-code login. `authorization_pending`/`slow_down` →
/// [`DevicePoll::Pending`]; `authorized` → [`DevicePoll::Ready`] with the
/// plaintext secret; anything else (expired/denied) → [`DevicePoll::Denied`].
pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<DevicePoll, ChannelError> {
    let url = format!(
        "{}/oauth/device/poll",
        DEFAULT_AUTH_BASE_URL.trim_end_matches('/')
    );
    let resp: DevicePollResponse = json_post(client, &url, &device_poll_body(device_code)).await?;

    match resp.status.as_deref() {
        Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
        Some("authorized") => Ok(DevicePoll::Ready(device_secret(resp)?)),
        _ => Ok(DevicePoll::Denied),
    }
}

/// Shape an `authorized` poll response into the plaintext secret the caller
/// seals + persists. `profile_arn` is omitted when absent; `provider` defaults
/// to `Github`; `expires_at_ms` is set an hour out (device tokens lack expiry).
fn device_secret(resp: DevicePollResponse) -> Result<Value, ChannelError> {
    let access_token = resp
        .access_token
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ChannelError::Build("device poll missing accessToken".into()))?;
    let refresh_token = resp
        .refresh_token
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ChannelError::Build("device poll missing refreshToken".into()))?;
    let provider = resp
        .identity_provider
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEVICE_LOGIN_PROVIDER.to_string());
    let expires_at_ms = now_ms() + DEVICE_TOKEN_TTL_MS;

    let mut secret = json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "provider": provider,
        "expires_at_ms": expires_at_ms,
    });
    if let Some(arn) = resp.profile_arn.filter(|s| !s.trim().is_empty()) {
        secret["profile_arn"] = Value::String(arn);
    }
    Ok(secret)
}

/// `{auth_base}/refreshToken` response (camelCase). Tolerant: every field
/// optional so a refresh that omits `refreshToken`/`profileArn` still parses.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    profile_arn: Option<String>,
    expires_in: Option<u64>,
}

/// Refresh a social credential: `POST {auth_base}/refreshToken` with
/// `{"refreshToken": rt}`. `access_token`/`expires_at_ms` rotate; `refresh_token`
/// and `profile_arn` rotate when present (else preserved); other fields carry
/// through the clone. `settings.auth_base_url` overrides the default host.
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?
        .to_string();
    let auth_base = secret_str(settings, "auth_base_url").unwrap_or(DEFAULT_AUTH_BASE_URL);
    let url = format!("{}/refreshToken", auth_base.trim_end_matches('/'));
    let body = json!({ "refreshToken": refresh_token });

    let resp: TokenResponse = json_post(client, &url, &body).await?;
    let new_access = resp
        .access_token
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing accessToken".into()))?;
    let expires_at_ms = now_ms() + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    // refresh_token ROTATES when present — store the new one, else keep the old.
    if let Some(rt) = resp.refresh_token.filter(|s| !s.trim().is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    // profile_arn is re-asserted on refresh — store it, else preserve.
    if let Some(arn) = resp.profile_arn.filter(|s| !s.trim().is_empty()) {
        obj.insert("profile_arn".into(), Value::String(arn));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(out)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn device_start_body_matches_capture() {
        // The captured kiro-cli-chat device-authorization request body.
        let body = device_start_body(login_provider(&json!({})));
        assert_eq!(body["clientId"], "Kiro-CLI");
        assert_eq!(body["loginProvider"], "Github");
        assert_eq!(
            body.as_object().unwrap().len(),
            2,
            "no extra fields beyond clientId + loginProvider"
        );
    }

    #[test]
    fn login_provider_selects_google_else_github() {
        assert_eq!(login_provider(&json!({})), "Github");
        assert_eq!(
            login_provider(&json!({ "login_provider": "google" })),
            "Google"
        );
        assert_eq!(
            login_provider(&json!({ "login_provider": "GitHub" })),
            "Github"
        );
        // `provider` alias + case-insensitive; unknown falls back to GitHub.
        assert_eq!(login_provider(&json!({ "provider": "GOOGLE" })), "Google");
        assert_eq!(
            login_provider(&json!({ "login_provider": "gitlab" })),
            "Github"
        );
    }

    #[test]
    fn device_poll_body_carries_device_code_and_client_id() {
        let body = device_poll_body("dev-code-xyz");
        assert_eq!(body["deviceCode"], "dev-code-xyz");
        assert_eq!(body["clientId"], "Kiro-CLI");
    }

    #[test]
    fn device_secret_maps_authorized_response() {
        let resp: DevicePollResponse = serde_json::from_value(json!({
            "status": "authorized",
            "accessToken": "at-1",
            "refreshToken": "rt-1",
            "profileArn": "arn:aws:kiro:profile/p1",
            "identityProvider": "Github",
        }))
        .unwrap();
        let secret = device_secret(resp).unwrap();
        assert_eq!(secret["access_token"], "at-1");
        assert_eq!(secret["refresh_token"], "rt-1");
        assert_eq!(secret["profile_arn"], "arn:aws:kiro:profile/p1");
        assert_eq!(secret["provider"], "Github");
        assert!(secret["expires_at_ms"].as_i64().unwrap() > crate::util::time::unix_now() * 1000);
    }

    #[test]
    fn device_secret_omits_absent_profile_arn_and_defaults_provider() {
        let resp: DevicePollResponse = serde_json::from_value(json!({
            "status": "authorized",
            "accessToken": "at-2",
            "refreshToken": "rt-2",
        }))
        .unwrap();
        let secret = device_secret(resp).unwrap();
        assert!(secret.get("profile_arn").is_none());
        assert_eq!(secret["provider"], "Github");
    }
}
