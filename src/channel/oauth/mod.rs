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
    /// Granted OAuth scopes, returned as a space-delimited string.
    pub scope: Option<String>,
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
/// `antigravity`. NOTE: `project_id` is NOT obtained by this token helper — each
/// channel performs Code Assist project resolution (`loadCodeAssist` /
/// `onboardUser`) as the following step before returning the minted secret.
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

/// A Code Assist project resolved during Google OAuth login.
pub struct GoogleProjectResolution {
    pub project_id: String,
    /// Normalized entitlement (`free`, `pro`, or `ultra`) when loadCodeAssist
    /// reports a paid/current tier.
    pub subscription_tier: Option<String>,
}

/// Resolve a Google Code Assist project via `v1internal:loadCodeAssist`,
/// falling back to `v1internal:onboardUser`. Shared by `geminicli` and
/// `antigravity`, which differ only in `metadata` (ideType/pluginType, optional
/// `duetProject`) and fallback `tier_id` (`legacy-tier` vs `LEGACY`). When
/// loadCodeAssist advertises a default allowed tier, that server-provided id is
/// used for onboarding instead. `existing` (an
/// operator-set project) is sent as `cloudaicompanionProject` and used as the
/// last-resort fallback. A pending `onboardUser` long-running operation is
/// polled just like the official Gemini CLI, using the dual-target runtime
/// timer so this works on native and wasm.
pub async fn resolve_google_project(
    client: &Arc<dyn UpstreamClient>,
    base_url: &str,
    access_token: &str,
    metadata: serde_json::Value,
    tier_id: &str,
    existing: Option<&str>,
    user_agent: Option<&str>,
) -> Result<GoogleProjectResolution, ChannelError> {
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
        user_agent,
    )
    .await?;
    let subscription_tier = google_subscription_tier(&loaded);
    if let Some(p) = loaded
        .get("cloudaicompanionProject")
        .and_then(google_project_from_value)
    {
        return Ok(GoogleProjectResolution {
            project_id: p,
            subscription_tier,
        });
    }

    // onboardUser (long-running op; read the immediate response)
    let tier_id = google_default_tier(&loaded).unwrap_or(tier_id);
    if existing.is_none() && google_tier_requires_user_project(&loaded, tier_id) {
        let reason = google_ineligible_tier_reason(&loaded)
            .map(|reason| format!("; upstream eligibility: {reason}"))
            .unwrap_or_default();
        return Err(ChannelError::Build(format!(
            "code assist tier {tier_id} requires a user-owned GCP project; supply project_id when starting login{reason}"
        )));
    }
    let mut onboard_body = json!({ "tierId": tier_id, "metadata": metadata });
    if let Some(p) = existing {
        onboard_body["cloudaicompanionProject"] = json!(p);
    }
    let mut onboarded = post_json_bearer(
        client,
        &format!("{base}/v1internal:onboardUser"),
        access_token,
        &onboard_body,
        user_agent,
    )
    .await?;
    if onboarded.get("done").and_then(serde_json::Value::as_bool) == Some(false)
        && let Some(name) = onboarded
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
    {
        onboarded = poll_google_operation(client, base, access_token, &name, user_agent).await?;
    }
    let project = onboarded
        .get("response")
        .and_then(|r| r.get("cloudaicompanionProject"))
        .and_then(google_project_from_value)
        .or_else(|| {
            onboarded
                .get("cloudaicompanionProject")
                .and_then(google_project_from_value)
        });
    let project_id = project
        .or_else(|| existing.map(ToOwned::to_owned))
        .ok_or_else(|| {
            tracing::warn!(
                "automatic code assist project resolution returned no project after onboarding; \
                 no project hint was supplied"
            );
            ChannelError::Build(
                "code assist project resolution returned no project (onboarding may be pending — \
                 retry or set project_id)"
                    .into(),
            )
        })?;
    Ok(GoogleProjectResolution {
        project_id,
        subscription_tier,
    })
}

const GOOGLE_OPERATION_POLL_INTERVAL_MS: u64 = 5_000;
const GOOGLE_OPERATION_MAX_POLLS: usize = 60;

async fn poll_google_operation(
    client: &Arc<dyn UpstreamClient>,
    base: &str,
    access_token: &str,
    name: &str,
    user_agent: Option<&str>,
) -> Result<serde_json::Value, ChannelError> {
    let operation_url = format!("{base}/v1internal/{}", name.trim_start_matches('/'));
    for _ in 0..GOOGLE_OPERATION_MAX_POLLS {
        crate::util::time::sleep_ms(GOOGLE_OPERATION_POLL_INTERVAL_MS).await;
        let operation = get_json_bearer(client, &operation_url, access_token, user_agent).await?;
        if operation.get("done").and_then(serde_json::Value::as_bool) != Some(false) {
            return Ok(operation);
        }
    }
    Err(ChannelError::Build(
        "code assist onboarding timed out waiting for project".into(),
    ))
}

fn google_default_tier(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("allowedTiers")?
        .as_array()?
        .iter()
        .find(|tier| tier.get("isDefault").and_then(serde_json::Value::as_bool) == Some(true))?
        .get("id")?
        .as_str()
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

fn google_tier_requires_user_project(payload: &serde_json::Value, tier_id: &str) -> bool {
    payload
        .get("allowedTiers")
        .and_then(serde_json::Value::as_array)
        .and_then(|tiers| {
            tiers.iter().find(|tier| {
                tier.get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| id.trim() == tier_id)
            })
        })
        .and_then(|tier| tier.get("userDefinedCloudaicompanionProject"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

fn google_ineligible_tier_reason(payload: &serde_json::Value) -> Option<String> {
    let tier = payload.get("ineligibleTiers")?.as_array()?.first()?;
    let code = tier
        .get("reasonCode")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|code| !code.is_empty());
    let message = tier
        .get("reasonMessage")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|message| !message.is_empty());
    match (code, message) {
        (Some(code), Some(message)) => Some(format!("{code}: {message}")),
        (Some(code), None) => Some(code.to_owned()),
        (None, Some(message)) => Some(message.to_owned()),
        (None, None) => None,
    }
}

fn google_subscription_tier(payload: &serde_json::Value) -> Option<String> {
    let tier_id = |key| {
        payload
            .get(key)
            .and_then(|tier| tier.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
    };
    let raw = tier_id("paidTier").or_else(|| tier_id("currentTier"))?;
    let normalized = match raw.to_ascii_lowercase().as_str() {
        "g1-ultra-tier" | "ws-ai-ultra-business-tier" => "ultra",
        "free-tier" => "free",
        _ => "pro",
    };
    Some(normalized.to_owned())
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
    user_agent: Option<&str>,
) -> Result<serde_json::Value, ChannelError> {
    let bytes = serde_json::to_vec(body)
        .map_err(|e| ChannelError::Build(format!("code assist body serialize: {e}")))?;
    let mut builder = http::Request::post(url)
        .header(http::header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json");
    if let Some(user_agent) = user_agent {
        builder = builder.header(http::header::USER_AGENT, user_agent);
    }
    let req = builder
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

async fn get_json_bearer(
    client: &Arc<dyn UpstreamClient>,
    url: &str,
    bearer: &str,
    user_agent: Option<&str>,
) -> Result<serde_json::Value, ChannelError> {
    let mut builder = http::Request::get(url)
        .header(http::header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::ACCEPT, "application/json");
    if let Some(user_agent) = user_agent {
        builder = builder.header(http::header::USER_AGENT, user_agent);
    }
    let req = builder
        .body(bytes::Bytes::new())
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

    #[test]
    fn parses_default_and_subscription_tiers() {
        let payload = serde_json::json!({
            "paidTier": {"id": "g1-ultra-tier"},
            "currentTier": {"id": "free-tier"},
            "allowedTiers": [
                {"id": "LEGACY"},
                {"id": "standard-tier", "isDefault": true}
            ]
        });
        assert_eq!(google_default_tier(&payload), Some("standard-tier"));
        assert_eq!(google_subscription_tier(&payload).as_deref(), Some("ultra"));

        let empty_paid = serde_json::json!({
            "paidTier": {"id": " "},
            "currentTier": {"id": "free-tier"}
        });
        assert_eq!(
            google_subscription_tier(&empty_paid).as_deref(),
            Some("free")
        );
    }

    #[test]
    fn detects_tiers_that_require_an_operator_project() {
        let payload = serde_json::json!({
            "allowedTiers": [
                {"id": "free-tier", "isDefault": false},
                {
                    "id": "standard-tier",
                    "isDefault": true,
                    "userDefinedCloudaicompanionProject": true
                }
            ],
            "ineligibleTiers": [{
                "reasonCode": "UNSUPPORTED_LOCATION",
                "reasonMessage": "not available in this location"
            }]
        });

        assert!(google_tier_requires_user_project(&payload, "standard-tier"));
        assert!(!google_tier_requires_user_project(&payload, "free-tier"));
        assert_eq!(
            google_ineligible_tier_reason(&payload).as_deref(),
            Some("UNSUPPORTED_LOCATION: not available in this location")
        );
    }
}
