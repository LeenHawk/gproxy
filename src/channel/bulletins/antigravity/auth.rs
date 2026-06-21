//! Antigravity auth — Google OAuth2 `refresh_token` grant against
//! `oauth2.googleapis.com/token`; base `https://cloudcode-pa.googleapis.com`.
//! Same Code Assist envelope and `/v1internal:` shape as `geminicli` (see
//! [`crate::channel::envelope`]); differs only in the OAuth client_id/secret
//! and the User-Agent.
//!
//! The login-time authorization-code + PKCE flow (6 scopes, remote-paste login,
//! `project_id` resolution via `loadCodeAssist` / `onboardUser`) is an M10
//! concern; this module covers only the per-request access-token use and the
//! refresh the pipeline drives. `project_id` is therefore expected to already
//! be present in the decrypted secret — a credential without it errors in
//! `prepare`.

use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::channel::ChannelError;
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

/// Public Antigravity OAuth client (the credentials the Antigravity app ships
/// with — distinct from the Gemini CLI client).
pub(super) const OAUTH_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
pub(super) const OAUTH_CLIENT_SECRET: &str = "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";
pub(super) const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Google authorization endpoint for the interactive authcode+PKCE login.
pub(super) const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Default redirect_uri the Antigravity login uses (mined from v1
/// `ANTIGRAVITY_REDIRECT_URI`) — a loopback callback listener (distinct port
/// from geminicli's).
pub(super) const DEFAULT_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
/// OAuth scopes requested at login (mined from v1 `ANTIGRAVITY_OAUTH_SCOPE` — a
/// superset of geminicli's, adding cclog / experimentsandconfigs / aicode).
pub(super) const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs https://www.googleapis.com/auth/aicode";

/// Code Assist API host (non-regional). Both verbs live under `/v1internal:`.
pub(super) const BASE_URL: &str = "https://cloudcode-pa.googleapis.com";

/// User-Agent the Antigravity app sends; some Code Assist paths key behaviour
/// off it (this is what distinguishes the channel from `geminicli` upstream).
pub(super) const USER_AGENT_VALUE: &str = "antigravity/cli/1.0.6 linux/amd64";

/// Refresh slightly before expiry to avoid racing a 401 mid-flight.
const EXPIRY_SKEW_MS: i64 = 60_000;

/// Read a trimmed, non-empty string field from the secret.
fn secret_str<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Build the authorize URL for the interactive authcode+PKCE login. An empty
/// `redirect_uri` falls back to [`DEFAULT_REDIRECT_URI`]. Returns the URL plus
/// the effective redirect_uri (so `complete` exchanges with the same value).
pub(super) fn authcode_start(redirect_uri: &str, state: &str, challenge: &str) -> (String, String) {
    let redirect_uri = if redirect_uri.trim().is_empty() {
        DEFAULT_REDIRECT_URI
    } else {
        redirect_uri
    };
    let url = oauth::google_authorize_url(
        AUTHORIZE_URL,
        OAUTH_CLIENT_ID,
        redirect_uri,
        OAUTH_SCOPE,
        state,
        challenge,
    );
    (url, redirect_uri.to_string())
}

/// Exchange a Google authcode (+PKCE verifier) for the plaintext secret. Same
/// `project_id` caveat as `geminicli`: the minted secret carries tokens but NO
/// `project_id` (Code Assist `loadCodeAssist` / `onboardUser` is a separate
/// step), so the operator must set it before `prepare` can address the API.
pub(super) async fn authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Value, ChannelError> {
    let mut secret = oauth::google_authcode_exchange(
        client,
        TOKEN_URL,
        OAUTH_CLIENT_ID,
        OAUTH_CLIENT_SECRET,
        code,
        verifier,
        redirect_uri,
    )
    .await?;
    let access_token = secret_str(&secret, "access_token")
        .ok_or_else(|| ChannelError::Build("token response missing access_token".into()))?
        .to_owned();
    let project_id = oauth::resolve_google_project_id(
        client,
        BASE_URL,
        &access_token,
        code_assist_metadata(),
        "LEGACY",
        None,
    )
    .await?;
    secret["project_id"] = Value::String(project_id);
    if let Some(email) = oauth::google_user_email(client, &access_token).await {
        secret["user_email"] = Value::String(email);
    }
    Ok(secret)
}

/// Antigravity Code Assist `metadata` — `ideType: ANTIGRAVITY`, no `duetProject`.
fn code_assist_metadata() -> Value {
    serde_json::json!({
        "ideType": "ANTIGRAVITY",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI"
    })
}

/// The OAuth access token, required by [`super::AntigravityChannel::prepare`].
pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))
}

/// The Code Assist `project_id`. Resolved at login by [`authcode_exchange`]
/// (via `resolve_google_project_id`) and required on every request, so a
/// credential that somehow lacks it cannot address the API.
pub(super) fn project_id(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "project_id")
        .ok_or_else(|| ChannelError::InvalidCredential("missing project_id (run login)".into()))
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
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms > expires_at_ms - EXPIRY_SKEW_MS
}

/// Refresh via the Google `refresh_token` grant, returning the new plaintext
/// secret (`access_token` + `expires_at_ms` rotate; `refresh_token` rotates when
/// the response carries one, else the old one is kept; every other field —
/// notably `project_id` — is preserved).
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;

    let form = [
        ("grant_type", "refresh_token"),
        ("client_id", OAUTH_CLIENT_ID),
        ("client_secret", OAUTH_CLIENT_SECRET),
        ("refresh_token", refresh_token),
    ];
    let resp = oauth::token_post(client, TOKEN_URL, &form, &[]).await?;

    let new_access = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut out = secret.clone();
    let obj = out
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    // refresh_token ROTATES when present — store the new one, else keep the old.
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(out)
}

/// Inject the OAuth bearer + Antigravity fingerprint headers onto the prepared
/// upstream request. The real `antigravity/cli` 1.0.6 inference call
/// (`:streamGenerateContent`) sends a MINIMAL set — `authorization`,
/// `content-type: application/json`, and the `User-Agent` — with NO `requestId`
/// / `requestType` / `Accept` (those, mined from an older build, are a tell). See
/// `docs/agent-request-headers.md`.
pub(super) fn apply(req: &mut Request<Bytes>, access_token: &str) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    Ok(())
}
