//! Shared OAuth helpers for the credential channels (PKCE, token exchange,
//! refresh). Channel-specific config (client_id, endpoints, scopes) lives in
//! each channel; this is the mechanical PKCE math + form-POST token exchange.
//!
//! Compiled on BOTH native and wasm: the edge build also refreshes OAuth
//! credentials. Randomness comes from `chacha20poly1305`'s `OsRng` (the same
//! source `crypto::envelope` seeds DEKs with — resolves getrandom's js backend
//! on wasm); the PKCE challenge is SHA-256 per the OAuth spec (RFC 7636).

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::channel::ChannelError;
use crate::http::client::UpstreamClient;
use crate::util::rand;

/// Generate a PKCE `(verifier, challenge)` pair (RFC 7636, S256). The verifier
/// is base64url(32 random bytes) → 43 chars (within the 43–128 spec range); the
/// challenge is base64url_nopad(SHA-256(verifier)).
pub fn pkce() -> (String, String) {
    let bytes = rand::bytes::<32>();
    let verifier = B64URL.encode(bytes);
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

/// OAuth token endpoint response. Tolerant: unknown fields are ignored, and
/// every field is optional so a refresh that omits `refresh_token` (the common
/// case — the existing one is reused) still parses.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
    /// OpenID Connect id_token (JWT). Surfaced for channels that decode claims
    /// from it (e.g. codex extracts the ChatGPT account id); ignored elsewhere.
    pub id_token: Option<String>,
}

/// POST `application/x-www-form-urlencoded` `form` pairs to `token_url` and
/// parse the JSON [`TokenResponse`]. Uses the passed [`UpstreamClient`] so the
/// call rides the proxy pool / edge transport. `extra_headers` are appended
/// (e.g. a `User-Agent` some providers require). Non-2xx → [`ChannelError::Build`]
/// carrying the status + (truncated) body.
pub async fn token_post(
    client: &Arc<dyn UpstreamClient>,
    token_url: &str,
    form: &[(&str, &str)],
    extra_headers: &[(&str, &str)],
) -> Result<TokenResponse, ChannelError> {
    let body = encode_form(form);
    let mut builder = http::Request::builder()
        .method(http::Method::POST)
        .uri(token_url)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json");
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = builder
        .body(bytes::Bytes::from(body))
        .map_err(|e| ChannelError::Build(format!("token request build: {e}")))?;

    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("token request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet = String::from_utf8_lossy(&body);
        let snippet: String = snippet.chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "token endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("token response parse: {e}")))
}

/// Encode `key=value` pairs as `application/x-www-form-urlencoded`. Both keys
/// and values are percent-encoded (RFC 3986 unreserved set kept verbatim).
fn encode_form(pairs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (k, v) in pairs {
        if !out.is_empty() {
            out.push('&');
        }
        percent_encode_into(k, &mut out);
        out.push('=');
        percent_encode_into(v, &mut out);
    }
    out
}

/// Build a Google OAuth2 authorize URL for an authcode+PKCE login, shared by the
/// `geminicli` and `antigravity` channels (same `accounts.google.com` endpoint,
/// differing only in client_id / scope / redirect_uri). Values are
/// percent-encoded. `access_type=offline` + `prompt=consent` ensure a
/// refresh_token is minted (mined from v1).
pub fn google_authorize_url(
    authorize_url: &str,
    client_id: &str,
    redirect_uri: &str,
    scope: &str,
    state: &str,
    challenge: &str,
) -> String {
    let query = [
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", scope),
        ("access_type", "offline"),
        ("prompt", "consent"),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge),
        ("state", state),
    ];
    let mut out = String::new();
    for (k, v) in query {
        out.push(if out.is_empty() { '?' } else { '&' });
        percent_encode_into(k, &mut out);
        out.push('=');
        percent_encode_into(v, &mut out);
    }
    format!("{authorize_url}{out}")
}

/// Exchange a Google authcode (+PKCE verifier) for the plaintext secret
/// `{access_token, refresh_token?, expires_at_ms}`, shared by `geminicli` and
/// `antigravity`. NOTE: `project_id` is NOT obtained here — Code Assist project
/// resolution (`loadCodeAssist` / `onboardUser`) is a separate step; the minted
/// secret carries tokens but no `project_id`, which the operator sets later (or
/// a follow-up adds resolution) before the channel can address the API.
pub async fn google_authcode_exchange(
    client: &Arc<dyn UpstreamClient>,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<serde_json::Value, ChannelError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code_verifier", verifier),
    ];
    let resp = token_post(client, token_url, &form, &[]).await?;

    let access_token = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("token response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;

    let mut secret = serde_json::json!({
        "access_token": access_token,
        "expires_at_ms": expires_at_ms,
    });
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        secret["refresh_token"] = serde_json::Value::String(rt);
    }
    Ok(secret)
}

/// Resolve a Google Code Assist `project_id` via `v1internal:loadCodeAssist`,
/// falling back to `v1internal:onboardUser`. Shared by `geminicli` and
/// `antigravity`, which differ only in `metadata` (ideType/pluginType, optional
/// `duetProject`) and `tier_id` (`legacy-tier` vs `LEGACY`). `existing` (an
/// operator-set project) is sent as `cloudaicompanionProject` and used as the
/// last-resort fallback. The `onboardUser` long-running operation is read once
/// (not polled across `sleep`, to stay wasm-compilable); a still-pending
/// onboarding without an immediate project falls back to `existing` or errors.
pub async fn resolve_google_project_id(
    client: &Arc<dyn UpstreamClient>,
    base_url: &str,
    access_token: &str,
    metadata: serde_json::Value,
    tier_id: &str,
    existing: Option<&str>,
) -> Result<String, ChannelError> {
    use serde_json::json;
    let base = base_url.trim_end_matches('/');
    let existing = existing.map(str::trim).filter(|s| !s.is_empty());

    // loadCodeAssist
    let mut load_body = json!({ "metadata": metadata });
    if let Some(p) = existing {
        load_body["cloudaicompanionProject"] = json!(p);
    }
    let loaded = post_json_bearer(
        client,
        &format!("{base}/v1internal:loadCodeAssist"),
        access_token,
        &load_body,
    )
    .await?;
    if let Some(p) = loaded
        .get("cloudaicompanionProject")
        .and_then(google_project_from_value)
    {
        return Ok(p);
    }

    // onboardUser (long-running op; read the immediate response)
    let mut onboard_body = json!({ "tierId": tier_id, "metadata": metadata });
    if let Some(p) = existing {
        onboard_body["cloudaicompanionProject"] = json!(p);
    }
    let onboarded = post_json_bearer(
        client,
        &format!("{base}/v1internal:onboardUser"),
        access_token,
        &onboard_body,
    )
    .await?;
    let project = onboarded
        .get("response")
        .and_then(|r| r.get("cloudaicompanionProject"))
        .and_then(google_project_from_value)
        .or_else(|| {
            onboarded
                .get("cloudaicompanionProject")
                .and_then(google_project_from_value)
        });
    project
        .or_else(|| existing.map(ToOwned::to_owned))
        .ok_or_else(|| {
            ChannelError::Build(
                "code assist project resolution returned no project (onboarding may be pending — \
                 retry or set project_id)"
                    .into(),
            )
        })
}

/// Extract a Code Assist project id from a value that is either the bare id
/// string or an object carrying `{ "id": "..." }`.
fn google_project_from_value(v: &serde_json::Value) -> Option<String> {
    v.as_str()
        .or_else(|| v.get("id").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// POST a JSON body with `Authorization: Bearer` and parse a 2xx JSON response.
/// Non-2xx → [`ChannelError::Build`] with status + a truncated snippet (never
/// the request body, which carries the bearer-scoped project metadata).
async fn post_json_bearer(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    bearer: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value, ChannelError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|e| ChannelError::Build(format!("code assist body serialize: {e}")))?;
    let req = http::Request::post(url)
        .header(http::header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json")
        .body(bytes::Bytes::from(bytes))
        .map_err(|e| ChannelError::Build(format!("code assist request build: {e}")))?;
    let resp = client
        .send(req)
        .await
        .map_err(|e| ChannelError::Build(format!("code assist request failed: {e}")))?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "code assist endpoint {}: {snippet}",
            parts.status
        )));
    }
    serde_json::from_slice(&body)
        .map_err(|e| ChannelError::Build(format!("code assist response parse: {e}")))
}

/// Fetch the Google userinfo endpoint and return the email address, if any.
/// Best-effort: returns `None` on any failure (the credential is still usable).
pub async fn google_user_email(
    client: &Arc<dyn UpstreamClient>,
    access_token: &str,
) -> Option<String> {
    const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v1/userinfo?alt=json";
    let req = http::Request::builder()
        .method(http::Method::GET)
        .uri(USERINFO_URL)
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .header(http::header::ACCEPT, "application/json")
        .body(bytes::Bytes::new())
        .ok()?;
    let resp = client.send(req).await.ok()?;
    let (parts, body) = resp.into_parts();
    if !parts.status.is_success() {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(&body).ok()?;
    payload
        .get("email")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

/// Percent-encode `s`, leaving the RFC 3986 unreserved set (`A-Za-z0-9-._~`)
/// verbatim and `%XX`-encoding every other byte. Exposed for channels that build
/// their own authorize URLs (e.g. Kiro SSO-OIDC / external-IdP).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    percent_encode_into(s, &mut out);
    out
}

/// Percent-encode `s` into `out`, leaving the RFC 3986 unreserved characters
/// (`A-Za-z0-9-._~`) as-is and `%XX`-encoding every other byte.
fn percent_encode_into(s: &str, out: &mut String) {
    for &b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(
                char::from_digit((b >> 4) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit((b & 0xf) as u32, 16)
                    .unwrap()
                    .to_ascii_uppercase(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PKCE challenge is exactly base64url_nopad(SHA-256(verifier)).
    #[test]
    fn pkce_challenge() {
        let (verifier, challenge) = pkce();
        let expected = B64URL.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
        // verifier within the RFC 7636 length range, base64url-safe alphabet.
        assert!((43..=128).contains(&verifier.len()));
        assert!(
            verifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        );
    }
}
