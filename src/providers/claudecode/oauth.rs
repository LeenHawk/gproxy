use axum::http::{HeaderValue, StatusCode, header};
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;
use wreq::{Client, Proxy};
use wreq_util::Emulation;

use crate::context::AppContext;
use crate::providers::claudecode::{
    CLAUDE_API_VERSION, CLAUDE_BETA_BASE, CLAUDE_CODE_CLIENT_ID,
    CLAUDE_CODE_REDIRECT_URI, ClaudeCodeCredential,
};
use crate::providers::credential_status::now_timestamp;

#[derive(Debug, Deserialize)]
struct BootstrapResponse {
    account: Option<BootstrapAccount>,
}

#[derive(Debug, Deserialize)]
struct BootstrapAccount {
    memberships: Vec<BootstrapMembership>,
}

#[derive(Debug, Deserialize)]
struct BootstrapMembership {
    organization: BootstrapOrganization,
}

#[derive(Debug, Deserialize)]
struct BootstrapOrganization {
    uuid: String,
    capabilities: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AuthorizeResponse {
    redirect_uri: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: Option<i64>,
}

pub(crate) struct ExchangedTokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

fn build_emulated_client(ctx: &AppContext) -> Result<Client, StatusCode> {
    let mut builder = Client::builder()
        .cookie_store(true)
        .emulation(Emulation::Chrome136);
    if let Some(proxy) = ctx.get_config().app.proxy.as_ref() {
        let proxy = Proxy::all(proxy.as_str()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub(crate) async fn exchange_session_key(
    ctx: &AppContext,
    credential: &ClaudeCodeCredential,
    base_url: &Url,
) -> Result<ExchangedTokens, StatusCode> {
    eprintln!("claudecode oauth exchange_session_key start");
    let cookie = format_session_cookie(&credential.session_key)?;
    let org_uuid = fetch_org_uuid(ctx, base_url, &cookie)
        .await
        .inspect_err(|status| {
            eprintln!("claudecode oauth fetch_org_uuid failed: status={}", status.as_u16());
        })?;
    let code_verifier = generate_code_verifier();
    let code_challenge = pkce_challenge(&code_verifier);
    let (code, state) =
        authorize_code(ctx, base_url, &cookie, &org_uuid, &code_challenge)
            .await
            .inspect_err(|status| {
                eprintln!("claudecode oauth authorize_code failed: status={}", status.as_u16());
            })?;
    exchange_token(ctx, base_url, &code_verifier, &code, state.as_deref())
        .await
        .inspect_err(|status| {
            eprintln!("claudecode oauth exchange_token failed: status={}", status.as_u16());
        })
}

fn format_session_cookie(session_key: &str) -> Result<String, StatusCode> {
    let raw = session_key.trim();
    if raw.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mut cleaned = if let Some(start) = raw.find("sessionKey=") {
        let value = &raw[start + "sessionKey=".len()..];
        value.split(';').next().unwrap_or(value).trim()
    } else {
        raw
    };
    if let Some(stripped) = cleaned.strip_prefix("sessionKey=") {
        cleaned = stripped.trim();
    }
    let cookie_value = if cleaned.starts_with("sk-ant-sid") {
        cleaned.to_string()
    } else {
        format!("sk-ant-sid01-{cleaned}")
    };
    Ok(format!("sessionKey={cookie_value}"))
}

async fn fetch_org_uuid(
    ctx: &AppContext,
    base_url: &Url,
    cookie: &str,
) -> Result<String, StatusCode> {
    let client = build_emulated_client(ctx)?;
    let referer = base_url
        .join("new")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let url = base_url
        .join("api/bootstrap")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let res = client
        .get(url.as_str())
        .header(header::COOKIE, HeaderValue::from_str(cookie).map_err(|_| StatusCode::BAD_REQUEST)?)
        .header(header::ORIGIN, base_url.as_str())
        .header(header::REFERER, referer.as_str())
        .header("anthropic-version", HeaderValue::from_static(CLAUDE_API_VERSION))
        .header("anthropic-beta", HeaderValue::from_static(CLAUDE_BETA_BASE))
        .send()
        .await
        .map_err(|err| {
            eprintln!("claudecode oauth bootstrap request failed: {err}");
            StatusCode::BAD_GATEWAY
        })?;
    let status = res.status();
    let body = res.bytes().await.map_err(|err| {
        eprintln!("claudecode oauth bootstrap read body failed: {err}");
        StatusCode::BAD_GATEWAY
    })?;
    let body_text = String::from_utf8_lossy(&body).to_string();
    if !status.is_success() {
        log_upstream_error("bootstrap", status, &body).await;
        return Err(StatusCode::BAD_GATEWAY);
    }
    let parsed: BootstrapResponse = serde_json::from_slice(&body).map_err(|_| {
        log_parse_error("bootstrap", &body);
        StatusCode::BAD_GATEWAY
    })?;
    let account = parsed.account.ok_or_else(|| {
        eprintln!("claudecode oauth bootstrap missing account: {body_text}");
        StatusCode::BAD_GATEWAY
    })?;
    let mut memberships = account.memberships.into_iter();
    let first = memberships.next().ok_or_else(|| {
        eprintln!("claudecode oauth bootstrap empty memberships: {body_text}");
        StatusCode::BAD_GATEWAY
    })?;
    let mut fallback = None;
    let candidates = std::iter::once(first).chain(memberships);
    for member in candidates {
        let caps = member.organization.capabilities.as_ref();
        let has_chat = caps
            .map(|caps| caps.iter().any(|cap| cap == "chat"))
            .unwrap_or(true);
        if !has_chat {
            continue;
        }
        if has_paid_capability(caps) {
            return Ok(member.organization.uuid);
        }
        if fallback.is_none() {
            fallback = Some(member.organization.uuid);
        }
    }
    if let Some(uuid) = fallback {
        eprintln!("claudecode oauth bootstrap: no paid org found, using fallback org");
        return Ok(uuid);
    }
    eprintln!("claudecode oauth bootstrap no chat capability: {body_text}");
    Err(StatusCode::BAD_GATEWAY)
}

async fn authorize_code(
    ctx: &AppContext,
    base_url: &Url,
    cookie: &str,
    org_uuid: &str,
    code_challenge: &str,
) -> Result<(String, Option<String>), StatusCode> {
    let client = build_emulated_client(ctx)?;
    let referer = base_url
        .join("new")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut payload = serde_json::Map::new();
    payload.insert("response_type".into(), "code".into());
    payload.insert("client_id".into(), CLAUDE_CODE_CLIENT_ID.into());
    payload.insert("redirect_uri".into(), CLAUDE_CODE_REDIRECT_URI.into());
    payload.insert("scope".into(), "user:profile user:inference".into());
    payload.insert("code_challenge".into(), code_challenge.into());
    payload.insert("code_challenge_method".into(), "S256".into());
    let state = generate_state();
    payload.insert("state".into(), state.clone().into());
    payload.insert("organization_uuid".into(), org_uuid.into());

    let url = base_url
        .join(&format!("v1/oauth/{org_uuid}/authorize"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let res = client
        .post(url.as_str())
        .header(header::COOKIE, HeaderValue::from_str(cookie).map_err(|_| StatusCode::BAD_REQUEST)?)
        .header(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(header::ORIGIN, base_url.as_str())
        .header(header::REFERER, referer.as_str())
        .header("anthropic-version", HeaderValue::from_static(CLAUDE_API_VERSION))
        .header("anthropic-beta", HeaderValue::from_static(CLAUDE_BETA_BASE))
        .body(serde_json::to_vec(&payload).map_err(|_| StatusCode::BAD_GATEWAY)?)
        .send()
        .await
        .map_err(|err| {
            eprintln!("claudecode oauth authorize request failed: {err}");
            StatusCode::BAD_GATEWAY
        })?;
    let status = res.status();
    let body = res.bytes().await.map_err(|err| {
        eprintln!("claudecode oauth authorize read body failed: {err}");
        StatusCode::BAD_GATEWAY
    })?;
    if !status.is_success() {
        log_upstream_error("authorize", status, &body).await;
        return Err(StatusCode::BAD_GATEWAY);
    }
    let parsed: AuthorizeResponse = serde_json::from_slice(&body).map_err(|_| {
        log_parse_error("authorize", &body);
        StatusCode::BAD_GATEWAY
    })?;
    let redirect = Url::parse(&parsed.redirect_uri).map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut code = None;
    let mut returned_state = None;
    for (key, value) in redirect.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => returned_state = Some(value.into_owned()),
            _ => {}
        }
    }
    let code = code.ok_or(StatusCode::BAD_GATEWAY)?;
    Ok((code, returned_state.or(Some(state))))
}

async fn exchange_token(
    ctx: &AppContext,
    base_url: &Url,
    code_verifier: &str,
    code: &str,
    state: Option<&str>,
) -> Result<ExchangedTokens, StatusCode> {
    let client = build_emulated_client(ctx)?;
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code");
        form.append_pair("client_id", CLAUDE_CODE_CLIENT_ID);
        form.append_pair("code", code);
        form.append_pair("code_verifier", code_verifier);
        form.append_pair("redirect_uri", CLAUDE_CODE_REDIRECT_URI);
        if let Some(state) = state {
            form.append_pair("state", state);
        }
        form.finish()
    };

    let url = base_url
        .join("v1/oauth/token")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let res = client
        .post(url.as_str())
        .header(header::CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .header("anthropic-version", HeaderValue::from_static(CLAUDE_API_VERSION))
        .header("anthropic-beta", HeaderValue::from_static(CLAUDE_BETA_BASE))
        .body(body)
        .send()
        .await
        .map_err(|err| {
            eprintln!("claudecode oauth token request failed: {err}");
            StatusCode::BAD_GATEWAY
        })?;
    let status = res.status();
    let bytes = res.bytes().await.map_err(|err| {
        eprintln!("claudecode oauth token read body failed: {err}");
        StatusCode::BAD_GATEWAY
    })?;
    if !status.is_success() {
        log_upstream_error("token", status, &bytes).await;
        return Err(StatusCode::BAD_GATEWAY);
    }
    let parsed: TokenResponse = serde_json::from_slice(&bytes).map_err(|_| {
        log_parse_error("token", &bytes);
        StatusCode::BAD_GATEWAY
    })?;
    let expires_at = now_timestamp()
        + parsed.expires_in.unwrap_or(0);
    Ok(ExchangedTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_at,
    })
}

async fn log_upstream_error(stage: &str, status: StatusCode, body: &[u8]) {
    let body = String::from_utf8_lossy(body).to_string();
    tracing::warn!(
        "claudecode oauth {} failed: status={}, body={}",
        stage,
        status.as_u16(),
        body
    );
    eprintln!(
        "claudecode oauth {} failed: status={}, body={}",
        stage,
        status.as_u16(),
        body
    );
}

fn log_parse_error(stage: &str, body: &[u8]) {
    let body = String::from_utf8_lossy(body).to_string();
    tracing::warn!("claudecode oauth {} parse failed: body={}", stage, body);
    eprintln!("claudecode oauth {} parse failed: body={}", stage, body);
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

fn has_paid_capability(caps: Option<&Vec<String>>) -> bool {
    let Some(caps) = caps else {
        return false;
    };
    caps.iter().any(|cap| {
        cap.contains("pro")
            || cap.contains("enterprise")
            || cap.contains("raven")
            || cap.contains("max")
    })
}
