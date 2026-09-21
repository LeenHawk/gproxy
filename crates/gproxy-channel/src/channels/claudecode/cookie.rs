//! Cookie login: a claude.ai `sessionKey` cookie is turned into the same
//! OAuth tokens the CLI holds (v3 `claudecode/cookie.rs`). The organization
//! with a subscription is read from `claude.ai/api/bootstrap`, an
//! authorization code is minted at `{base}/v1/oauth/{org}/authorize` with a
//! PKCE pair the channel generates, and the code is exchanged at
//! `{base}/v1/oauth/token` as a form (this older endpoint, unlike
//! platform.claude.com, is what the cookie-issued code is valid for).

use super::{
    ClaudecodeConfig, TokenResponse, base_url, credential_from_tokens, enrich_profile, form_encode,
    invalid_response, previous_fields, previous_scopes, secret_json, send,
};
use crate::OutboundClient;
use crate::channel::{
    ChannelError, CredentialUpdate, OAuthCredential, ProviderView, RefreshContext,
};
use base64::Engine;
use gproxy_protocol::connection::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Cloudflare challenges on claude.ai are retried this many times.
const MAX_ATTEMPTS: u32 = 5;
const SUBSCRIPTION_CAPABILITIES: &[&str] = &[
    "claude_pro",
    "claude_max",
    "claude_team",
    "claude_enterprise",
];

/// Accepts a full `Cookie:` header, a `sessionKey=...` list or a bare
/// `sk-ant-sid...` key; the result is a cookie header value
/// (v3 `shared/claude/cookie.rs`).
pub fn normalize(input: &str) -> Option<String> {
    let mut text = input.trim();
    if let Some((name, value)) = text.split_once(':')
        && name.trim().eq_ignore_ascii_case("cookie")
    {
        text = value.trim();
    }
    let session_key = text.split(';').find_map(|part| {
        part.trim()
            .strip_prefix("sessionKey=")
            .map(str::trim)
            .filter(|value| value.starts_with("sk-ant-sid"))
    });
    let session_key = session_key.or_else(|| {
        (text.starts_with("sk-ant-sid") && !text.contains(['=', ';'])).then_some(text)
    })?;
    if !text.contains("sessionKey=") {
        return Some(format!("sessionKey={session_key}"));
    }
    let pairs = text
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty() && part.contains('='))
        .collect::<Vec<_>>();
    (!pairs.is_empty()).then(|| pairs.join("; "))
}

/// Mint an OAuth credential from a cookie; returns it with the normalized
/// cookie so the caller can keep the cookie in the secret.
pub(super) async fn exchange(
    client: &dyn OutboundClient,
    provider: ProviderView<'_>,
    config: &ClaudecodeConfig,
    input: &str,
) -> Result<(OAuthCredential, String), ChannelError> {
    let cookie = normalize(input).ok_or(ChannelError::InvalidCredential)?;
    let base = base_url(provider);
    let claude_ai = config.claude_ai_url.trim_end_matches('/');
    let organization = discover_organization(client, claude_ai, &cookie).await?;
    let (verifier, challenge, state) = pkce()?;
    let code = authorize(
        client,
        &base,
        claude_ai,
        config,
        &cookie,
        &organization,
        &state,
        &challenge,
    )
    .await?;
    let tokens = token_exchange(client, &base, claude_ai, config, &verifier, &state, &code).await?;
    let mut fields = BTreeMap::new();
    fields.insert("account_uuid".into(), Value::String(organization));
    let mut credential = credential_from_tokens(tokens, None, Vec::new(), fields)?;
    enrich_profile(
        client,
        &base,
        &credential.access_token,
        &mut credential.provider_fields,
    )
    .await;
    Ok((credential, cookie))
}

/// A cookie-only credential refreshes by minting again; facts the earlier
/// login recorded survive when the new profile lacks them.
pub(super) async fn refresh(
    context: RefreshContext<'_>,
    config: &ClaudecodeConfig,
    cookie: &str,
) -> Result<CredentialUpdate, ChannelError> {
    let secret = context.credential.secret;
    let (mut credential, cookie) = exchange(context.client, context.provider, config, cookie)
        .await
        .map_err(|error| match error {
            ChannelError::InvalidCredential => {
                ChannelError::RefreshRejected("cookie is not a claude.ai session".into())
            }
            other => other,
        })?;
    for (key, value) in previous_fields(secret) {
        credential.provider_fields.entry(key).or_insert(value);
    }
    if credential.scopes.is_empty() {
        credential.scopes = previous_scopes(secret);
    }
    Ok(CredentialUpdate {
        expires_at_ms: credential.expires_at_ms,
        secret: secret_json(&credential, Some(&cookie))?,
    })
}

fn browser_headers(claude_ai: &str, cookie: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        HeaderValue::from_str(cookie).map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_str(claude_ai)
            .map_err(|_| ChannelError::InvalidConfig("claude_ai_url".into()))?,
    );
    Ok(headers)
}

async fn discover_organization(
    client: &dyn OutboundClient,
    claude_ai: &str,
    cookie: &str,
) -> Result<String, ChannelError> {
    let mut headers = browser_headers(claude_ai, cookie)?;
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::ACCEPT_LANGUAGE,
        HeaderValue::from_static("en-US,en;q=0.9"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(
        header::REFERER,
        HeaderValue::from_str(&format!("{claude_ai}/new"))
            .map_err(|_| ChannelError::InvalidConfig("claude_ai_url".into()))?,
    );
    let body = send_ok(
        client,
        Method::GET,
        &format!("{claude_ai}/api/bootstrap"),
        &headers,
        None,
    )
    .await?;
    let value = parse_bootstrap(&body)?;
    value
        .pointer("/account/memberships")
        .and_then(Value::as_array)
        .and_then(|memberships| {
            memberships
                .iter()
                .filter_map(|membership| membership.get("organization"))
                .find(|organization| has_subscription(organization))
        })
        .and_then(|organization| organization.get("uuid"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| invalid_response("cookie has no subscription-capable organization"))
}

#[allow(clippy::too_many_arguments)]
async fn authorize(
    client: &dyn OutboundClient,
    base: &str,
    claude_ai: &str,
    config: &ClaudecodeConfig,
    cookie: &str,
    organization: &str,
    state: &str,
    challenge: &str,
) -> Result<String, ChannelError> {
    let payload = json!({
        "response_type": "code",
        "client_id": config.client_id,
        "organization_uuid": organization,
        "redirect_uri": super::DEFAULT_REDIRECT_URI,
        "scope": super::OAUTH_SCOPE,
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    });
    let mut headers = browser_headers(claude_ai, cookie)?;
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(super::ANTHROPIC_VERSION),
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(super::OAUTH_BETA),
    );
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::CLI_USER_AGENT),
    );
    let body = send_ok(
        client,
        Method::POST,
        &format!("{base}/v1/oauth/{organization}/authorize"),
        &headers,
        Some(payload.to_string().into_bytes()),
    )
    .await?;
    let response: Value = serde_json::from_slice(&body)
        .map_err(|error| invalid_response(format!("invalid authorize response: {error}")))?;
    response
        .get("redirect_uri")
        .and_then(Value::as_str)
        .and_then(|uri| query_parameter(uri, "code"))
        .ok_or_else(|| invalid_response("authorize response has no code"))
}

async fn token_exchange(
    client: &dyn OutboundClient,
    base: &str,
    claude_ai: &str,
    config: &ClaudecodeConfig,
    verifier: &str,
    state: &str,
    code: &str,
) -> Result<TokenResponse, ChannelError> {
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("client_id", &config.client_id),
        ("code", code),
        ("redirect_uri", super::DEFAULT_REDIRECT_URI),
        ("code_verifier", verifier),
        ("state", state),
    ]);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(super::ANTHROPIC_VERSION),
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(super::OAUTH_BETA),
    );
    headers.insert(
        header::ORIGIN,
        HeaderValue::from_str(claude_ai)
            .map_err(|_| ChannelError::InvalidConfig("claude_ai_url".into()))?,
    );
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(super::CLI_USER_AGENT),
    );
    let (status, _, bytes) = send(
        client,
        Method::POST,
        &format!("{base}/v1/oauth/token"),
        headers,
        Some(body.into_bytes()),
    )
    .await?;
    if !status.is_success() {
        return Err(ChannelError::UpstreamResponse {
            status,
            body: bytes,
        });
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| invalid_response(format!("invalid token response: {error}")))
}

/// Send until a success; a Cloudflare challenge is retried, anything else
/// is returned as the upstream's reply.
async fn send_ok(
    client: &dyn OutboundClient,
    method: Method,
    url: &str,
    headers: &HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<Bytes, ChannelError> {
    let mut last = None;
    for _ in 0..MAX_ATTEMPTS {
        let (status, _, bytes) =
            send(client, method.clone(), url, headers.clone(), body.clone()).await?;
        if status.is_success() {
            return Ok(bytes);
        }
        if !is_cloudflare_challenge(status, &bytes) {
            return Err(ChannelError::UpstreamResponse {
                status,
                body: bytes,
            });
        }
        last = Some((status, bytes));
    }
    let (status, body) = last.unwrap_or((StatusCode::FORBIDDEN, Bytes::new()));
    Err(ChannelError::UpstreamResponse { status, body })
}

/// claude.ai answers bootstrap with a stream of JSON documents; the one
/// carrying `account` is the answer.
fn parse_bootstrap(body: &[u8]) -> Result<Value, ChannelError> {
    let mut first = None;
    for value in serde_json::Deserializer::from_slice(body)
        .into_iter::<Value>()
        .flatten()
    {
        if value.get("account").and_then(Value::as_object).is_some() {
            return Ok(value);
        }
        first.get_or_insert(value);
    }
    first.ok_or_else(|| invalid_response("bootstrap response is empty"))
}

fn has_subscription(organization: &Value) -> bool {
    organization
        .get("capabilities")
        .and_then(Value::as_array)
        .is_some_and(|capabilities| {
            capabilities
                .iter()
                .filter_map(Value::as_str)
                .any(|value| SUBSCRIPTION_CAPABILITIES.contains(&value))
        })
}

fn is_cloudflare_challenge(status: StatusCode, body: &[u8]) -> bool {
    if !matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::SERVICE_UNAVAILABLE
    ) {
        return false;
    }
    let text = String::from_utf8_lossy(&body[..body.len().min(1_024)]);
    ["Just a moment", "challenge-platform", "cf-chl", "cf_chl"]
        .iter()
        .any(|marker| text.contains(marker))
}

fn query_parameter(uri: &str, name: &str) -> Option<String> {
    uri.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_owned())
    })
}

/// A fresh PKCE verifier, its S256 challenge and a state, all base64url.
fn pkce() -> Result<(String, String, String), ChannelError> {
    let mut verifier = [0_u8; 32];
    let mut state = [0_u8; 24];
    getrandom::fill(&mut verifier)
        .and_then(|()| getrandom::fill(&mut state))
        .map_err(|_| ChannelError::InvalidConfig("secure randomness unavailable".into()))?;
    let engine = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let verifier = engine.encode(verifier);
    let challenge = engine.encode(Sha256::digest(verifier.as_bytes()));
    let state = engine.encode(state);
    Ok((verifier, challenge, state))
}
