//! Claude Code cookie bootstrap (§14.5): mint OAuth tokens from a claude.ai
//! `sessionKey` cookie. Ported from v1 `utils/claudecode_cookie.rs`, adapted to
//! the [`UpstreamClient`] transport (no `wreq`, no metadata tracking).
//!
//! Flow: `sessionKey` cookie → `/api/bootstrap` org discovery → `/v1/oauth/{org}/authorize`
//! (PKCE) → `/v1/oauth/token` exchange. The client_id / scope / redirect_uri
//! MUST match the main channel's authcode flow — Anthropic validates the triple
//! at the authorize step. The minted secret is
//! `{access_token, refresh_token?, expires_at_ms, cookie}`; the cookie is kept
//! so a later operator can re-bootstrap, and so the channel can fall back to it.

use std::sync::Arc;

use bytes::Bytes;
use http::header::{ACCEPT, CONTENT_TYPE};
use http::{Request, Response};
use serde_json::Value;

use super::auth::{CLAUDE_AI_BASE_URL, DEFAULT_REDIRECT_URI, OAUTH_CLIENT_ID, OAUTH_SCOPE};
use crate::channel::ChannelError;
use crate::channel::oauth;
use crate::http::client::UpstreamClient;

const API_BASE: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const USER_AGENT: &str = "claude-code/2.1.199";

/// claude.ai is Cloudflare-fronted and intermittently answers with a managed
/// "Just a moment…" challenge instead of the real response. The challenge is
/// roughly a per-call coin-flip and a non-browser client cannot solve it, so the
/// Cloudflare-facing requests are simply retried (a fresh connection gets an
/// independent roll). At 5 attempts the residual failure is ~3%.
const COOKIE_MAX_ATTEMPTS: u32 = 5;

/// Org capabilities that gate Claude Code OAuth (`user:inference` scope).
/// API-only orgs return `permission_error` at the authorize step, so the
/// subscription-capable membership is selected.
const SUBSCRIPTION_CAPS: &[&str] = &[
    "claude_pro",
    "claude_max",
    "claude_team",
    "claude_enterprise",
];

/// Bootstrap an OAuth secret from a claude.ai session cookie. See module docs
/// for the flow. The plaintext secret is returned for the caller to seal.
pub(super) async fn exchange(
    client: &Arc<dyn UpstreamClient>,
    cookie: &str,
) -> Result<Value, ChannelError> {
    let session_key = normalize_session_key(cookie)
        .ok_or_else(|| ChannelError::InvalidCredential("missing sessionKey".into()))?;

    let org_uuid = discover_org(client, &session_key).await?;
    let (verifier, challenge) = oauth::pkce();
    let state = crate::util::rand::uuid_v4();
    let code = authorize(client, &session_key, &org_uuid, &state, &challenge).await?;
    let secret = token_exchange(client, &verifier, &state, &code).await?;

    let mut secret = secret;
    if let Some(obj) = secret.as_object_mut() {
        obj.insert("cookie".into(), Value::String(session_key));
        obj.insert("account_uuid".into(), Value::String(org_uuid));
    }
    super::auth::enrich_from_profile(client, &mut secret).await;
    super::auth::ensure_device_id(&mut secret);
    Ok(secret)
}

/// Re-mint a secret from the stored `cookie` (§14.5 M7b): the cookie-only
/// refresh path for a credential that carries no `refresh_token`. Uses the
/// pipeline's resolved `client` — it already carries this credential's
/// `(proxy, Chrome-emulation)` profile (the channel's `default_emulation` when no
/// DB fingerprint override), so it clears Cloudflare-fronted `claude.ai` AND
/// egresses through the configured proxy (no self-built client, which would
/// bypass the proxy). Overlays the freshly minted token/cookie/account fields
/// onto the existing secret so operator fields the bootstrap never sets
/// (device_id / user_email …) survive the refresh.
pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let cookie = secret
        .get("cookie")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::InvalidCredential("missing cookie".into()))?;
    let minted = exchange(client, cookie).await?;
    Ok(overlay(secret, &minted))
}

/// Overlay the freshly minted bootstrap secret onto the existing one: minted
/// token/cookie/account fields win; any other field already present (device_id /
/// user_email …) is preserved.
fn overlay(old: &Value, minted: &Value) -> Value {
    let mut out = old.clone();
    if let (Some(obj), Some(m)) = (out.as_object_mut(), minted.as_object()) {
        for (k, v) in m {
            obj.insert(k.clone(), v.clone());
        }
    }
    out
}

/// Accept the Console's `sessionKey=...`, a full Cookie header, or the bare
/// `sk-ant-sid...` value, and return the bare session key. Older stored secrets
/// may also carry `sessionKey=...`; refresh passes through here too.
fn normalize_session_key(input: &str) -> Option<String> {
    let mut text = input.trim();
    if text.is_empty() {
        return None;
    }
    if let Some((name, rest)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = rest.trim();
    }
    for part in text.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("sessionKey=") {
            let value = value.trim();
            if value.starts_with("sk-ant-sid") {
                return Some(value.to_owned());
            }
        }
    }
    if text.starts_with("sk-ant-sid") && !text.contains('=') && !text.contains(';') {
        return Some(text.to_owned());
    }
    None
}

fn session_cookie_header(session_key: &str) -> String {
    format!("sessionKey={session_key}")
}

/// Step 1: GET `/api/bootstrap`, pick the first subscription-capable org uuid.
async fn discover_org(
    client: &Arc<dyn UpstreamClient>,
    session_key: &str,
) -> Result<String, ChannelError> {
    let body = send_ok(client, "bootstrap", || {
        cookie_get(format!("{CLAUDE_AI_BASE_URL}/api/bootstrap"), session_key)
    })
    .await?;
    // claude.ai may prepend a usage object before the bootstrap payload; scan
    // the JSON value stream for the one carrying `account`.
    let value = parse_bootstrap(&body)?;
    let org = value
        .get("account")
        .and_then(|a| a.get("memberships"))
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .filter_map(|m| m.get("organization"))
                .find(|o| org_has_subscription(o))
        })
        .and_then(|o| o.get("uuid"))
        .and_then(Value::as_str)
        .map(str::to_string);
    org.ok_or_else(|| {
        ChannelError::Build(
            "cookie auth: no subscription-capable organization (claude_pro/max/team/enterprise)"
                .into(),
        )
    })
}

/// Step 2: POST `/v1/oauth/{org}/authorize` with PKCE, extract `code` from the
/// returned `redirect_uri`.
async fn authorize(
    client: &Arc<dyn UpstreamClient>,
    session_key: &str,
    org_uuid: &str,
    state: &str,
    challenge: &str,
) -> Result<String, ChannelError> {
    let payload = serde_json::json!({
        "response_type": "code",
        "client_id": OAUTH_CLIENT_ID,
        "organization_uuid": org_uuid,
        "redirect_uri": DEFAULT_REDIRECT_URI,
        "scope": OAUTH_SCOPE,
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    });
    let body = serde_json::to_vec(&payload)
        .map_err(|e| ChannelError::Build(format!("authorize payload: {e}")))?;
    let url = format!("{API_BASE}/v1/oauth/{org_uuid}/authorize");
    let resp = send_ok(client, "authorize", || {
        Request::post(url.as_str())
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("cookie", session_cookie_header(session_key))
            .header("origin", CLAUDE_AI_BASE_URL)
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", OAUTH_BETA)
            .header(http::header::USER_AGENT, USER_AGENT)
            .body(Bytes::from(body.clone()))
            .map_err(|e| ChannelError::Build(format!("authorize request build: {e}")))
    })
    .await?;
    let value: Value = serde_json::from_slice(&resp)
        .map_err(|e| ChannelError::Build(format!("authorize response parse: {e}")))?;
    let redirect = value
        .get("redirect_uri")
        .and_then(Value::as_str)
        .ok_or_else(|| ChannelError::Build("authorize: missing redirect_uri".into()))?;
    query_param(redirect, "code")
        .ok_or_else(|| ChannelError::Build("authorize: missing code in redirect_uri".into()))
}

/// Step 3: POST `/v1/oauth/token` with the code + verifier + state, map to the
/// `{access_token, refresh_token?, expires_at_ms}` secret.
async fn token_exchange(
    client: &Arc<dyn UpstreamClient>,
    verifier: &str,
    state: &str,
    code: &str,
) -> Result<Value, ChannelError> {
    let form = [
        ("grant_type", "authorization_code"),
        ("client_id", OAUTH_CLIENT_ID),
        ("code", code),
        ("redirect_uri", DEFAULT_REDIRECT_URI),
        ("code_verifier", verifier),
        ("state", state),
    ];
    let extra = [
        ("anthropic-version", API_VERSION),
        ("anthropic-beta", OAUTH_BETA),
        ("origin", CLAUDE_AI_BASE_URL),
        ("user-agent", USER_AGENT),
    ];
    let resp = super::auth::token_post(client, &form, &extra).await?;
    let access_token = resp
        .access_token
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ChannelError::Build("cookie token response missing access_token".into()))?;
    let expires_at_ms = crate::util::time::unix_now().saturating_mul(1000)
        + resp.expires_in.unwrap_or(3600) as i64 * 1000;
    let mut secret = serde_json::json!({
        "access_token": access_token,
        "expires_at_ms": expires_at_ms,
    });
    if let Some(rt) = resp.refresh_token.filter(|s| !s.is_empty()) {
        secret["refresh_token"] = Value::String(rt);
    }
    Ok(secret)
}

fn org_has_subscription(org: &Value) -> bool {
    org.get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|caps| {
            caps.iter()
                .filter_map(Value::as_str)
                .any(|s| SUBSCRIPTION_CAPS.contains(&s))
        })
}

/// Prefer the JSON-stream value carrying `account`; else the first value.
fn parse_bootstrap(body: &[u8]) -> Result<Value, ChannelError> {
    let stream = serde_json::Deserializer::from_slice(body).into_iter::<Value>();
    let mut first = None;
    for value in stream.flatten() {
        if value.get("account").and_then(Value::as_object).is_some() {
            return Ok(value);
        }
        if first.is_none() {
            first = Some(value);
        }
    }
    first.ok_or_else(|| ChannelError::Build("bootstrap: empty response".into()))
}

fn cookie_get(url: String, session_key: &str) -> Result<Request<Bytes>, ChannelError> {
    Request::get(url)
        .header(ACCEPT, "application/json")
        .header("accept-language", "en-US,en;q=0.9")
        .header("cache-control", "no-cache")
        .header("cookie", session_cookie_header(session_key))
        .header("origin", CLAUDE_AI_BASE_URL)
        .header("referer", format!("{CLAUDE_AI_BASE_URL}/new"))
        .body(Bytes::new())
        .map_err(|e| ChannelError::Build(format!("cookie request build: {e}")))
}

/// Send a Cloudflare-fronted request and return the 2xx body. `build` is invoked
/// once per attempt — `send` consumes the request body, so it must be rebuildable.
/// A Cloudflare "Just a moment…" managed challenge ([`is_cloudflare_challenge`])
/// is retried up to [`COOKIE_MAX_ATTEMPTS`] times with a short backoff; any other
/// non-2xx fails at once with the status + a body snippet (the cookie rides the
/// header, never the logged form).
async fn send_ok<F>(
    client: &Arc<dyn UpstreamClient>,
    what: &str,
    build: F,
) -> Result<Bytes, ChannelError>
where
    F: Fn() -> Result<Request<Bytes>, ChannelError>,
{
    let mut last_challenge: Option<(http::StatusCode, Bytes)> = None;
    for attempt in 0..COOKIE_MAX_ATTEMPTS {
        let resp: Response<Bytes> = client
            .send(build()?)
            .await
            .map_err(|e| ChannelError::Build(format!("{what} request failed: {e}")))?;
        let (parts, body) = resp.into_parts();
        if parts.status.is_success() {
            return Ok(body);
        }
        if is_cloudflare_challenge(parts.status, &body) {
            // A fresh connection on retry gets an independent challenge roll; a
            // short increasing backoff avoids hammering the edge.
            crate::util::time::sleep_ms(200 * (attempt as u64 + 1)).await;
            last_challenge = Some((parts.status, body));
            continue;
        }
        let snippet: String = String::from_utf8_lossy(&body).chars().take(256).collect();
        return Err(ChannelError::Build(format!(
            "{what} endpoint {}: {snippet}",
            parts.status
        )));
    }
    let (status, body) = last_challenge.expect("loop only continues on a challenge");
    let snippet: String = String::from_utf8_lossy(&body).chars().take(160).collect();
    Err(ChannelError::Build(format!(
        "{what} blocked by Cloudflare challenge after {COOKIE_MAX_ATTEMPTS} attempts ({status}): {snippet}"
    )))
}

/// Recognise Cloudflare's interstitial managed-challenge response (the
/// "Just a moment…" page) so it can be retried rather than surfaced as a hard
/// failure. Served as 403 (sometimes 503) with tell-tale challenge HTML.
fn is_cloudflare_challenge(status: http::StatusCode, body: &[u8]) -> bool {
    use http::StatusCode as S;
    if status != S::FORBIDDEN && status != S::SERVICE_UNAVAILABLE {
        return false;
    }
    let head = String::from_utf8_lossy(&body[..body.len().min(1024)]);
    head.contains("Just a moment")
        || head.contains("challenge-platform")
        || head.contains("cf-chl")
        || head.contains("cf_chl")
}

fn query_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn overlay_refreshes_tokens_and_preserves_operator_fields() {
        // Cookie-only secret pre-refresh, carrying an operator field the
        // bootstrap never sets.
        let old = json!({
            "access_token": "stale",
            "cookie": "sessionKey-abc",
            "device_id": "dev-1",
        });
        // What the bootstrap mints (fresh tokens + account, same cookie).
        let minted = json!({
            "access_token": "fresh",
            "refresh_token": "rt-new",
            "expires_at_ms": 42,
            "cookie": "sessionKey-abc",
            "account_uuid": "org-9",
        });

        let out = overlay(&old, &minted);
        assert_eq!(out["access_token"], "fresh"); // minted wins
        assert_eq!(out["refresh_token"], "rt-new"); // minted adds it
        assert_eq!(out["account_uuid"], "org-9");
        assert_eq!(out["device_id"], "dev-1"); // operator field survives
    }

    #[test]
    fn cloudflare_just_a_moment_is_a_retryable_challenge() {
        // The interstitial HTML 403 → retryable.
        let page = br#"<!DOCTYPE html><html><head><title>Just a moment...</title>"#;
        assert!(is_cloudflare_challenge(http::StatusCode::FORBIDDEN, page));
        // A genuine permission error (JSON 403) is NOT a challenge — it must
        // surface immediately rather than burn retries.
        let perm = br#"{"type":"error","error":{"type":"permission_error"}}"#;
        assert!(!is_cloudflare_challenge(http::StatusCode::FORBIDDEN, perm));
        // A success is never a challenge.
        assert!(!is_cloudflare_challenge(http::StatusCode::OK, page));
    }
}
