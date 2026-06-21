//! Gemini CLI auth — Google OAuth2 `refresh_token` grant against
//! `oauth2.googleapis.com/token`; base `https://cloudcode-pa.googleapis.com`.
//! Requests are wrapped in the Code Assist envelope (see
//! [`crate::channel::envelope`]).
//!
//! The login-time authorization-code + PKCE flow is HEADLESS / code-only: the
//! authorize URL targets the Gemini CLI's own no-browser redirect
//! (`https://codeassist.google.com/authcode`), Google renders the code, and the
//! operator pastes it back — no loopback server. Same public CLI client_id /
//! client_secret / scopes as the official `code_assist/oauth2.ts`. The Code
//! Assist `project_id` is resolved at exchange time via `loadCodeAssist` /
//! `onboardUser`; `prepare` then requires it on every request.

use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::channel::ChannelError;
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

/// Public Gemini CLI OAuth client (the credentials the official CLI ships with).
pub(super) const OAUTH_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
pub(super) const OAUTH_CLIENT_SECRET: &str = "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";
pub(super) const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Google authorization endpoint for the interactive authcode+PKCE login.
pub(super) const AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Default redirect_uri for the HEADLESS / code-only login. This is the Gemini
/// CLI's own no-browser redirect (`authWithUserCode` in the sample
/// `code_assist/oauth2.ts`): Google renders the authorization code on a page the
/// operator copies, so NO loopback server is needed. The operator pastes the
/// code (or the full callback URL) back into `/admin/login-flows/complete`.
///
/// An operator who *does* run the CLI's loopback listener can override this with
/// `http://127.0.0.1:<port>/oauth2callback` via the `redirect_uri` request hint.
pub(super) const DEFAULT_REDIRECT_URI: &str = "https://codeassist.google.com/authcode";
/// Loopback redirect for the CALLBACK-URL login: the Gemini CLI's localhost
/// listener (`code_assist/oauth2.ts`, the browser flow). The operator (or the
/// console) catches `?code=…` on this URL and auto-completes. Selected with
/// `params.code_only = false`; the operator may also pass any registered
/// loopback `redirect_uri` directly.
pub(super) const LOOPBACK_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth2callback";
/// OAuth scopes requested at login (mined from v1 `GEMINICLI_OAUTH_SCOPE`).
pub(super) const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";

/// Code Assist API host (non-regional). Both verbs live under `/v1internal:`.
pub(super) const BASE_URL: &str = "https://cloudcode-pa.googleapis.com";

/// Gemini CLI model-path User-Agent; `model` = the requested model id (e.g.
/// `gemini-2.5-pro`), which the real CLI embeds. See
/// `docs/agent-tls-fingerprints.md` §5. Some Code Assist paths key off this.
pub(super) fn user_agent(model: &str) -> String {
    format!("GeminiCLI-tui/0.46.0/{model} (linux; x64; terminal) google-api-nodejs-client/9.15.1")
}
/// `x-goog-api-client` on the model path is just the Node runtime tag (the real
/// CLI sends `gl-node/<nodeversion>`, no genai-sdk prefix).
const GOOG_API_CLIENT: &str = "gl-node/22.20.0";

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

/// Exchange a Google authcode (+PKCE verifier) for the plaintext secret
/// `{access_token, refresh_token?, expires_at_ms, project_id}`. The Code Assist
/// `project_id` is resolved via `loadCodeAssist` / `onboardUser` so `prepare`
/// can address the API without the operator setting it manually.
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
        code_assist_metadata(None),
        "legacy-tier",
        None,
    )
    .await?;
    secret["project_id"] = Value::String(project_id);
    if let Some(email) = oauth::google_user_email(client, &access_token).await {
        secret["user_email"] = Value::String(email);
    }
    Ok(secret)
}

/// Gemini CLI Code Assist `metadata` (mirrors the genai SDK fingerprint). The
/// `duetProject` is only present when an operator project is already known.
fn code_assist_metadata(project: Option<&str>) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("ideType".into(), Value::String("IDE_UNSPECIFIED".into()));
    m.insert(
        "platform".into(),
        Value::String("PLATFORM_UNSPECIFIED".into()),
    );
    m.insert("pluginType".into(), Value::String("GEMINI".into()));
    if let Some(p) = project.map(str::trim).filter(|s| !s.is_empty()) {
        m.insert("duetProject".into(), Value::String(p.into()));
    }
    Value::Object(m)
}

/// The OAuth access token, required by [`super::GeminiCliChannel::prepare`].
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
    apply_token_response(secret.clone(), resp)
}

/// Fold a token-endpoint [`oauth::TokenResponse`] into the existing secret:
/// `access_token` + `expires_at_ms` rotate; `refresh_token` rotates only when
/// the response carries one (else the stored one is kept); every other field —
/// notably `project_id` — is preserved. Split out from [`refresh`] so the
/// mapping is unit-testable without a live token endpoint.
fn apply_token_response(
    mut secret: Value,
    resp: oauth::TokenResponse,
) -> Result<Value, ChannelError> {
    let new_access = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let obj = secret
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    obj.insert("access_token".into(), Value::String(new_access));
    // refresh_token ROTATES when present — store the new one, else keep the old.
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        obj.insert("refresh_token".into(), Value::String(rt));
    }
    obj.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(secret)
}

/// Inject the OAuth bearer + Gemini CLI fingerprint headers onto the prepared
/// upstream request. `content-type`/`accept` are forced to `application/json`
/// (the stream still arrives as SSE — that is selected by `alt=sse`, not Accept,
/// matching the Gemini CLI's own request).
pub(super) fn apply(
    req: &mut Request<Bytes>,
    access_token: &str,
    model: &str,
) -> Result<(), ChannelError> {
    let bearer = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|e| ChannelError::InvalidCredential(format!("bad access_token: {e}")))?;
    let h = req.headers_mut();
    h.insert(AUTHORIZATION, bearer);
    h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    h.insert(ACCEPT, HeaderValue::from_static("*/*"));
    let ua = HeaderValue::from_str(&user_agent(model))
        .map_err(|e| ChannelError::Build(format!("bad user-agent: {e}")))?;
    h.insert(USER_AGENT, ua);
    h.insert(
        HeaderName::from_static("x-goog-api-client"),
        HeaderValue::from_static(GOOG_API_CLIENT),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The headless / code-only authorize URL: empty redirect_uri falls back to
    /// the Gemini CLI's own no-browser redirect, and the URL carries the public
    /// CLI client_id, the exact Code Assist scopes, and the PKCE S256 challenge —
    /// matching the sample `code_assist/oauth2.ts` `authWithUserCode`.
    #[test]
    fn authcode_start_builds_headless_authorize_url() {
        let (url, redirect_uri) = authcode_start("", "st-123", "chal-xyz");

        // Empty hint → the no-loopback codeassist redirect, echoed back so
        // `complete` exchanges with the same value.
        assert_eq!(redirect_uri, "https://codeassist.google.com/authcode");

        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("response_type=code"));
        // `.` is in the RFC 3986 unreserved set → kept verbatim, not %-encoded.
        assert!(url.contains(&format!("client_id={OAUTH_CLIENT_ID}")));
        // redirect_uri is percent-encoded (`:` `/` encoded; `.` kept verbatim).
        assert!(url.contains("redirect_uri=https%3A%2F%2Fcodeassist.google.com%2Fauthcode"));
        // The three Code Assist scopes (space-separated → %20).
        assert!(url.contains("cloud-platform%20https"));
        assert!(url.contains("userinfo.email"));
        assert!(url.contains("userinfo.profile"));
        // Offline + consent so a refresh_token is minted; PKCE S256.
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("code_challenge=chal-xyz"));
        assert!(url.contains("state=st-123"));
    }

    /// A non-empty redirect_uri hint (e.g. an operator running the CLI's loopback
    /// listener) overrides the headless default and is echoed back verbatim.
    #[test]
    fn authcode_start_honors_redirect_override() {
        let (url, redirect_uri) = authcode_start("http://127.0.0.1:1455/oauth2callback", "s", "c");
        assert_eq!(redirect_uri, "http://127.0.0.1:1455/oauth2callback");
        // `.` kept verbatim; `:` `/` percent-encoded.
        assert!(url.contains("127.0.0.1%3A1455%2Foauth2callback"));
    }

    /// `refresh` preserves every non-rotating secret field (notably `project_id`)
    /// when the token endpoint omits a new `refresh_token`. Drives the mapping
    /// through a stubbed token response.
    #[test]
    fn refresh_keeps_old_refresh_token_and_preserves_project_id() {
        let secret = serde_json::json!({
            "access_token": "old-at",
            "refresh_token": "keep-rt",
            "project_id": "proj-42",
            "expires_at_ms": 1,
        });
        let resp = crate::channel::oauth::TokenResponse {
            access_token: Some("new-at".into()),
            refresh_token: None,
            expires_in: Some(3600),
            id_token: None,
        };
        let out = apply_token_response(secret, resp).unwrap();
        assert_eq!(out["access_token"], "new-at");
        assert_eq!(out["refresh_token"], "keep-rt");
        assert_eq!(out["project_id"], "proj-42");
        assert!(out["expires_at_ms"].as_i64().unwrap() > 1);
    }

    /// A rotated `refresh_token` replaces the stored one.
    #[test]
    fn refresh_rotates_refresh_token_when_present() {
        let secret = serde_json::json!({
            "access_token": "old-at",
            "refresh_token": "old-rt",
            "project_id": "p",
        });
        let resp = crate::channel::oauth::TokenResponse {
            access_token: Some("new-at".into()),
            refresh_token: Some("new-rt".into()),
            expires_in: Some(1800),
            id_token: None,
        };
        let out = apply_token_response(secret, resp).unwrap();
        assert_eq!(out["refresh_token"], "new-rt");
    }
}
