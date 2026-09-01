use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, ChannelError, ClientProfile, ClientProfilePreset, RequiredClientProfile, SimpleHttp,
};
use http::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{account, auth};

const COOKIE_MAX_ATTEMPTS: u32 = 5;
const SUBSCRIPTION_CAPABILITIES: &[&str] = &[
    "claude_pro",
    "claude_max",
    "claude_team",
    "claude_enterprise",
];
static BROWSER_PROFILE: ClientProfile = ClientProfile::preset(ClientProfilePreset::Chrome148);

pub(super) fn exchange<'a>(
    http: &'a dyn SimpleHttp,
    input: &'a str,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    Box::pin(async move {
        let cookie = normalize_cookie(input)
            .ok_or_else(|| ChannelError::Login("cookie is missing sessionKey".into()))?;
        let organization = discover_organization(http, &cookie).await?;
        let (verifier, challenge, state) = pkce()?;
        let code = authorize(http, &cookie, &organization, &state, &challenge).await?;
        let mut secret = token_exchange(http, &verifier, &state, &code).await?;
        secret["cookie"] = Value::String(cookie);
        secret["account_uuid"] = Value::String(organization);
        account::enrich(http, &mut secret).await;
        ensure_device_id(&mut secret);
        Ok(secret)
    })
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let cookie = secret
        .get("cookie")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Box::pin(async move {
        let cookie = cookie.ok_or_else(|| ChannelError::Refresh("cookie missing".into()))?;
        let minted = exchange(http, cookie)
            .await
            .map_err(|error| ChannelError::Refresh(error.to_string()))?;
        Ok(overlay(secret, &minted))
    })
}

fn normalize_cookie(input: &str) -> Option<String> {
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

async fn discover_organization(
    http: &dyn SimpleHttp,
    cookie: &str,
) -> Result<String, ChannelError> {
    let body = send_ok(http, "bootstrap", || {
        let request = http::Request::get(format!("{}/api/bootstrap", auth::CLAUDE_AI_BASE_URL))
            .header(ACCEPT, "application/json")
            .header("accept-language", "en-US,en;q=0.9")
            .header("cache-control", "no-cache")
            .header("cookie", cookie)
            .header("origin", auth::CLAUDE_AI_BASE_URL)
            .header("referer", format!("{}/new", auth::CLAUDE_AI_BASE_URL))
            .body(Bytes::new())
            .map_err(|error| ChannelError::Login(error.to_string()))?;
        Ok(browser_request(request))
    })
    .await?;
    let value = parse_bootstrap(&body)?;
    value
        .get("account")
        .and_then(|account| account.get("memberships"))
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
        .ok_or_else(|| {
            ChannelError::Login("cookie has no subscription-capable organization".into())
        })
}

async fn authorize(
    http: &dyn SimpleHttp,
    cookie: &str,
    organization: &str,
    state: &str,
    challenge: &str,
) -> Result<String, ChannelError> {
    let payload = json!({
        "response_type": "code",
        "client_id": auth::CLIENT_ID,
        "organization_uuid": organization,
        "redirect_uri": auth::DEFAULT_REDIRECT_URI,
        "scope": auth::OAUTH_SCOPE,
        "state": state,
        "code_challenge": challenge,
        "code_challenge_method": "S256",
    });
    let body =
        serde_json::to_vec(&payload).map_err(|error| ChannelError::Login(error.to_string()))?;
    let uri = format!(
        "{}/v1/oauth/{organization}/authorize",
        auth::DEFAULT_BASE_URL
    );
    let response = send_ok(http, "authorize", || {
        let request = http::Request::post(&uri)
            .header(CONTENT_TYPE, "application/json")
            .header(ACCEPT, "application/json")
            .header("cookie", cookie)
            .header("origin", auth::CLAUDE_AI_BASE_URL)
            .header("anthropic-version", auth::ANTHROPIC_VERSION)
            .header("anthropic-beta", auth::OAUTH_BETA)
            .header(USER_AGENT, auth::CLI_USER_AGENT)
            .body(Bytes::copy_from_slice(&body))
            .map_err(|error| ChannelError::Login(error.to_string()))?;
        Ok(browser_request(request))
    })
    .await?;
    let response: Value = serde_json::from_slice(&response)
        .map_err(|error| ChannelError::Login(format!("invalid authorize response: {error}")))?;
    response
        .get("redirect_uri")
        .and_then(Value::as_str)
        .and_then(|uri| query_parameter(uri, "code"))
        .ok_or_else(|| ChannelError::Login("authorize response missing code".into()))
}

async fn token_exchange(
    http: &dyn SimpleHttp,
    verifier: &str,
    state: &str,
    code: &str,
) -> Result<Value, ChannelError> {
    let body = crate::shared::http::form(&[
        ("grant_type", "authorization_code"),
        ("client_id", auth::CLIENT_ID),
        ("code", code),
        ("redirect_uri", auth::DEFAULT_REDIRECT_URI),
        ("code_verifier", verifier),
        ("state", state),
    ]);
    let request = http::Request::post(auth::TOKEN_URL)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json, text/plain, */*")
        .header("anthropic-version", auth::ANTHROPIC_VERSION)
        .header("anthropic-beta", auth::OAUTH_BETA)
        .header("origin", auth::CLAUDE_AI_BASE_URL)
        .header(USER_AGENT, auth::CLI_USER_AGENT)
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Login(error.to_string()))?;
    let response = http.send(browser_request(request)).await?;
    if !response.status().is_success() {
        return Err(endpoint_error("token", response.status(), response.body()));
    }
    let token: Value = serde_json::from_slice(response.body())
        .map_err(|error| ChannelError::Login(format!("invalid token response: {error}")))?;
    let access_token = required(&token, "access_token")?;
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600)
        .max(0);
    let mut secret = json!({
        "access_token": access_token,
        "expires_at_ms": auth::unix_now_ms().saturating_add(expires_in.saturating_mul(1_000)),
    });
    if let Some(refresh_token) = token
        .get("refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        secret["refresh_token"] = Value::String(refresh_token.into());
    }
    if let Some(scope) = token.get("scope").and_then(Value::as_str) {
        secret["scopes"] = Value::Array(
            scope
                .split_whitespace()
                .map(|value| Value::String(value.into()))
                .collect(),
        );
    }
    Ok(secret)
}

async fn send_ok<F>(http: &dyn SimpleHttp, name: &str, build: F) -> Result<Bytes, ChannelError>
where
    F: Fn() -> Result<http::Request<Bytes>, ChannelError>,
{
    let mut challenge = None;
    for _ in 0..COOKIE_MAX_ATTEMPTS {
        let response = http.send(build()?).await?;
        if response.status().is_success() {
            return Ok(response.into_body());
        }
        if is_cloudflare_challenge(response.status(), response.body()) {
            challenge = Some((response.status(), response.into_body()));
            continue;
        }
        return Err(endpoint_error(name, response.status(), response.body()));
    }
    let (status, body) = challenge.expect("only challenges exhaust the retry loop");
    Err(endpoint_error(name, status, &body))
}

fn browser_request(mut request: http::Request<Bytes>) -> http::Request<Bytes> {
    request.extensions_mut().insert(BROWSER_PROFILE.clone());
    request.extensions_mut().insert(RequiredClientProfile);
    request
}

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
    first.ok_or_else(|| ChannelError::Login("bootstrap response is empty".into()))
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

fn is_cloudflare_challenge(status: http::StatusCode, body: &[u8]) -> bool {
    if !matches!(
        status,
        http::StatusCode::FORBIDDEN | http::StatusCode::SERVICE_UNAVAILABLE
    ) {
        return false;
    }
    let text = String::from_utf8_lossy(&body[..body.len().min(1_024)]);
    ["Just a moment", "challenge-platform", "cf-chl", "cf_chl"]
        .iter()
        .any(|marker| text.contains(marker))
}

fn endpoint_error(name: &str, status: http::StatusCode, body: &[u8]) -> ChannelError {
    let snippet = String::from_utf8_lossy(body)
        .chars()
        .take(256)
        .collect::<String>();
    ChannelError::Login(format!("{name} endpoint {status}: {snippet}"))
}

fn query_parameter(uri: &str, name: &str) -> Option<String> {
    uri.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| value.to_owned())
    })
}

fn pkce() -> Result<(String, String, String), ChannelError> {
    let mut verifier = [0_u8; 32];
    let mut state = [0_u8; 24];
    getrandom::fill(&mut verifier)
        .and_then(|()| getrandom::fill(&mut state))
        .map_err(|_| ChannelError::Login("secure randomness unavailable".into()))?;
    let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state);
    Ok((verifier, challenge, state))
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Login(format!("token response missing {name}")))
}

fn ensure_device_id(secret: &mut Value) {
    if secret.get("device_id").and_then(Value::as_str).is_none() {
        secret["device_id"] = Value::String(auth::device_id(secret));
    }
}

fn overlay(old: &Value, minted: &Value) -> Value {
    let mut output = old.clone();
    if let (Some(output), Some(minted)) = (output.as_object_mut(), minted.as_object()) {
        for (key, value) in minted {
            output.insert(key.clone(), value.clone());
        }
    }
    output
}

#[cfg(test)]
mod tests;
