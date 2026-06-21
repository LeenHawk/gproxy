//! Kiro auth — multi-method credential acquisition + dispatching refresh.
//!
//! Kiro (a fork of the Amazon Q Developer CLI) exposes three login methods in
//! its `login` menu, confirmed by reverse-engineering `kiro-cli`
//! (`crates/fig_auth`). Each lives in its own submodule:
//!   * **social** ([`social`]) — GitHub / Google device-code via the Kiro desktop
//!     auth service (`*.auth.desktop.kiro.dev`).
//!   * **builderId** / **idc** ([`sso_oidc`]) — AWS SSO-OIDC (Builder ID, or IAM
//!     Identity Center / "Your Organization") authcode + PKCE against
//!     `oidc.{region}.amazonaws.com`.
//!
//! (kiro-cli also has a portal-config-driven `external_idp` kind, but it is not a
//! `login`-menu item, so v2 does not implement it.)
//!
//! The interactive flows are dispatched by `params.auth_method`
//! ([`authcode_start`], default `builderId`); [`refresh`] dispatches on the
//! stored secret:
//!   * `client_id` + `client_secret` present → SSO-OIDC CreateToken refresh
//!   * else                                  → social `/refreshToken`
//!
//! Social + SSO-OIDC use [`json_post`] (JSON bodies).

mod social;
mod sso_oidc;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::channel::ChannelError;
use crate::channel::login::{AuthCodeStart, DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

/// Begin a social (GitHub / Google) device-code login — see [`social::device_start`].
pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
) -> Result<DeviceInit, ChannelError> {
    social::device_start(client, params).await
}

/// Poll a pending social device-code login — see [`social::device_poll`].
pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    device_code: &str,
) -> Result<DevicePoll, ChannelError> {
    social::device_poll(client, device_code).await
}

/// User-agent the captured `kiro-cli-chat` auth requests send; harmless on the
/// AWS SSO-OIDC host too. Used by [`json_post`] for ALL JSON auth calls.
const AUTH_USER_AGENT: &str = "Kiro-CLI";
/// Refresh slightly before expiry to avoid racing a 401 mid-flight.
const EXPIRY_SKEW_MS: i64 = 60_000;

/// Read a trimmed, non-empty string field from a JSON object.
fn secret_str<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Current unix time in milliseconds (i64, matching the `expires_at_ms` secret
/// field).
fn now_ms() -> i64 {
    crate::util::time::unix_now().saturating_mul(1000)
}

/// SSO-OIDC (builderId / idc) when both `client_id` and `client_secret` are
/// present — they ride the registered-client creds; social tokens do not.
fn is_sso_oidc(secret: &Value) -> bool {
    secret_str(secret, "client_id").is_some() && secret_str(secret, "client_secret").is_some()
}

/// The access token (Bearer), required by [`super::KiroChannel::prepare`].
pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))
}

/// The CodeWhisperer `profile_arn`, lifted into the Smithy body when present
/// (secret takes precedence over the provider default).
pub(super) fn profile_arn<'a>(secret: &'a Value, settings: &'a Value) -> Option<&'a str> {
    secret_str(secret, "profile_arn").or_else(|| secret_str(settings, "profile_arn"))
}

/// Whether the access token is absent or within the skew window of expiry.
pub(super) fn needs_refresh(secret: &Value) -> bool {
    if secret_str(secret, "access_token").is_none() {
        return true;
    }
    let expires_at_ms = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    // `expires_at_ms == 0` means "unknown" → treat as valid; the 401-driven
    // refresh path still covers stale tokens.
    if expires_at_ms == 0 {
        return false;
    }
    now_ms() > expires_at_ms - EXPIRY_SKEW_MS
}

/// Begin an interactive authcode+PKCE login, dispatched by `params.auth_method`:
/// `builderId` (default) / `idc` → [`sso_oidc`]. `social` uses the device flow
/// ([`device_start`]); it (and any unknown method) is rejected here rather than
/// silently triggering an AWS RegisterClient.
pub(super) async fn authcode_start(
    client: &Arc<dyn UpstreamClient>,
    params: &Value,
    redirect_uri: &str,
    state: &str,
    pkce_challenge: &str,
) -> Result<AuthCodeStart, ChannelError> {
    match params.get("auth_method").and_then(Value::as_str) {
        Some("builderId") | Some("idc") | None => {
            sso_oidc::authcode_start(client, params, redirect_uri, state, pkce_challenge).await
        }
        Some(other) => Err(ChannelError::Build(format!(
            "kiro authcode_start: unsupported auth_method '{other}' \
             (social uses the device flow)"
        ))),
    }
}

/// Exchange an authcode for the plaintext secret (AWS SSO-OIDC). `extra` is the
/// login-session state stashed at [`authcode_start`] (the registered client
/// creds + region + start_url).
pub(super) async fn authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    extra: Option<&Value>,
) -> Result<Value, ChannelError> {
    let extra =
        extra.ok_or_else(|| ChannelError::Build("login session missing channel state".into()))?;
    sso_oidc::authcode_exchange(client, code, verifier, redirect_uri, extra).await
}

/// Refresh the credential, dispatched on the stored secret shape: `client_id` +
/// `client_secret` present → SSO-OIDC (builderId / idc) CreateToken refresh;
/// else the social `/refreshToken` (`settings.auth_base_url` reaches it).
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    secret: &Value,
) -> Result<Value, ChannelError> {
    if is_sso_oidc(secret) {
        sso_oidc::refresh(client, secret).await
    } else {
        social::refresh(client, settings, secret).await
    }
}

/// POST a JSON `body` to `url` and parse the response. Mirrors
/// [`oauth::token_post`](crate::channel::oauth::token_post) but with a JSON body
/// (the Kiro / AWS SSO-OIDC endpoints take JSON, not form-urlencoded). Rides the
/// passed [`UpstreamClient`] (proxy pool / edge transport). Non-2xx →
/// [`ChannelError::Build`] with the status + (truncated) body. Shared by
/// [`social`] and [`sso_oidc`].
async fn json_post<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    body: &Value,
) -> Result<T, ChannelError> {
    let payload = serde_json::to_vec(body)
        .map_err(|e| ChannelError::Build(format!("kiro auth request serialize: {e}")))?;
    let req = http::Request::builder()
        .method(http::Method::POST)
        .uri(url)
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json")
        .header(http::header::USER_AGENT, AUTH_USER_AGENT)
        .body(Bytes::from(payload))
        .map_err(|e| ChannelError::Build(format!("kiro auth request build: {e}")))?;

    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("kiro auth request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "kiro auth endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("kiro auth response parse: {e}")))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn is_sso_oidc_requires_both_client_creds() {
        assert!(is_sso_oidc(
            &json!({ "client_id": "c", "client_secret": "s" })
        ));
        assert!(!is_sso_oidc(&json!({ "client_id": "c" })));
        assert!(!is_sso_oidc(&json!({ "refresh_token": "rt" })));
        // A social token (refresh_token, optional profile_arn, no client creds)
        // is NOT sso_oidc — refresh routes it to the social path.
        assert!(!is_sso_oidc(
            &json!({ "refresh_token": "rt", "profile_arn": "arn" })
        ));
    }

    #[test]
    fn needs_refresh_honors_skew_and_unknown_expiry() {
        assert!(needs_refresh(&json!({})), "no access_token → refresh");
        assert!(
            !needs_refresh(&json!({ "access_token": "a" })),
            "unknown expiry → valid"
        );
        assert!(
            !needs_refresh(&json!({ "access_token": "a", "expires_at_ms": now_ms() + 600_000 })),
            "far future → valid"
        );
        assert!(
            needs_refresh(&json!({ "access_token": "a", "expires_at_ms": now_ms() + 1_000 })),
            "within skew → refresh"
        );
    }
}
