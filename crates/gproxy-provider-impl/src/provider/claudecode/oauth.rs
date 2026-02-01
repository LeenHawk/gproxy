use std::collections::HashMap;
use std::sync::OnceLock;

use base64::Engine;
use http::header::{CONTENT_TYPE, USER_AGENT};
use http::{HeaderMap, HeaderValue, StatusCode};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use sha2::Digest;
use time::OffsetDateTime;
use tracing::warn;

use gproxy_provider_core::{
    CredentialEntry, CredentialPool, PoolSnapshot, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_storage::AdminCredentialInput;

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::storage::global_storage;

use super::{
    channel_urls, headers_to_json, CLAUDE_CODE_UA, CLIENT_ID, OAUTH_SCOPE, OAUTH_SCOPE_SETUP,
    PROVIDER_NAME, TOKEN_UA,
};

const OAUTH_STATE_TTL_SECS: i64 = 600;

#[derive(Clone, Debug)]
struct OAuthState {
    code_verifier: String,
    redirect_uri: String,
    created_at: OffsetDateTime,
    scope: String,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthStartQuery {
    redirect_uri: Option<String>,
    scope: Option<String>,
    setup: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponseRaw {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
    subscription: Option<JsonValue>,
    plan: Option<JsonValue>,
    tier: Option<JsonValue>,
    account_type: Option<JsonValue>,
}

#[derive(Debug, Clone)]
pub(super) struct OAuthTokens {
    pub(super) access_token: String,
    pub(super) refresh_token: Option<String>,
    pub(super) expires_at: Option<i64>,
    pub(super) scopes: Vec<String>,
    pub(super) email: Option<String>,
    pub(super) subscription_type: Option<String>,
    pub(super) rate_limit_tier: Option<String>,
    pub(super) session_key: Option<String>,
}

#[derive(Debug)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

static OAUTH_STATES: OnceLock<tokio::sync::RwLock<HashMap<String, OAuthState>>> = OnceLock::new();

pub(super) async fn handle_oauth_start(
    query: Option<String>,
    headers: HeaderMap,
    ctx: UpstreamContext,
) -> Result<super::UpstreamOk, UpstreamPassthroughError> {
    let channel = channel_urls(&ctx).await?;
    let params: OAuthStartQuery = parse_query(query.as_deref())?;
    let redirect_uri = params
        .redirect_uri
        .unwrap_or_else(|| format!("{}/oauth/code/callback", channel.console_base));
    let scope = if params.setup.unwrap_or(false) {
        OAUTH_SCOPE_SETUP.to_string()
    } else {
        params.scope.unwrap_or_else(|| OAUTH_SCOPE.to_string())
    };
    let (state_id, pkce) = generate_state_and_pkce();
    let auth_url = build_authorize_url(
        &channel.claude_ai_base,
        &redirect_uri,
        &pkce.code_challenge,
        &state_id,
        &scope,
    );

    let mut guard = oauth_states().write().await;
    prune_oauth_states(&mut guard);
    guard.insert(
        state_id.clone(),
        OAuthState {
            code_verifier: pkce.code_verifier,
            redirect_uri: redirect_uri.clone(),
            created_at: OffsetDateTime::now_utc(),
            scope,
        },
    );

    let body = json!({
        "auth_url": auth_url,
        "state": state_id,
        "redirect_uri": redirect_uri,
    });
    let response = json_response(body)?;
    let meta = UpstreamRecordMeta {
        provider: PROVIDER_NAME.to_string(),
        provider_id: ctx.provider_id,
        credential_id: None,
        operation: "claudecode.oauth".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/claudecode/oauth".to_string(),
        request_query: query,
        request_headers: headers_to_json(&headers),
        request_body: String::new(),
    };
    Ok(super::UpstreamOk { response, meta })
}

pub(super) async fn handle_oauth_callback(
    pool: &CredentialPool<BaseCredential>,
    query: Option<String>,
    headers: HeaderMap,
    ctx: UpstreamContext,
) -> Result<super::UpstreamOk, UpstreamPassthroughError> {
    let params: OAuthCallbackQuery = parse_query(query.as_deref())?;
    if let Some(error) = params.error {
        let message = params.error_description.unwrap_or(error);
        warn!(event = "claudecode.oauth_callback", error = %message);
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            message,
        ));
    }
    let Some(code) = params.code else {
        warn!(event = "claudecode.oauth_callback", error = "missing code");
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            "missing code",
        ));
    };
    let state_param = params.state.clone();
    let oauth_state = {
        let mut guard = oauth_states().write().await;
        prune_oauth_states(&mut guard);
        match params.state {
            Some(ref state_id) => guard.remove(state_id),
            None => {
                if guard.len() == 1 {
                    let key = guard.keys().next().cloned();
                    key.and_then(|state_id| guard.remove(&state_id))
                } else {
                    None
                }
            }
        }
    };
    let Some(oauth_state) = oauth_state else {
        warn!(event = "claudecode.oauth_callback", error = "missing state");
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            "missing state (multiple or no pending oauth states)",
        ));
    };

    let channel = channel_urls(&ctx).await?;
    let mut tokens = exchange_code_for_tokens(
        &oauth_state.redirect_uri,
        &oauth_state.code_verifier,
        &code,
        oauth_state.scope.as_str(),
        state_param.as_deref(),
        ctx.proxy.as_deref(),
        &channel.api_base,
        &channel.claude_ai_base,
    )
    .await?;
    enrich_with_profile(&mut tokens, ctx.proxy.as_deref(), &channel.api_base)
        .await
        .ok();
    tokens.session_key = None;

    persist_claudecode_credential(
        pool,
        ctx.provider_id,
        None,
        tokens.clone(),
    )
    .await?;

    let body = json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "expires_at": tokens.expires_at,
        "email": tokens.email,
        "plan": tokens.subscription_type,
        "rate_limit_tier": tokens.rate_limit_tier,
    });
    let response = json_response(body)?;
    let meta = UpstreamRecordMeta {
        provider: PROVIDER_NAME.to_string(),
        provider_id: ctx.provider_id,
        credential_id: None,
        operation: "claudecode.oauth_callback".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/claudecode/oauth/callback".to_string(),
        request_query: query,
        request_headers: headers_to_json(&headers),
        request_body: String::new(),
    };
    Ok(super::UpstreamOk { response, meta })
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn exchange_code_for_tokens(
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    scope: &str,
    state: Option<&str>,
    proxy: Option<&str>,
    api_base: &str,
    claude_ai_base: &str,
) -> Result<OAuthTokens, UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let cleaned_code = code.split('#').next().unwrap_or(code);
    let cleaned_code = cleaned_code.split('&').next().unwrap_or(cleaned_code);
    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("client_id", CLIENT_ID.to_string()),
        ("code", cleaned_code.to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("code_verifier", code_verifier.to_string()),
    ];
    if let Some(state) = state {
        form.push(("state", state.to_string()));
    }
    let payload = form
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, urlencoding::encode(&value)))
        .collect::<Vec<_>>()
        .join("&");
    let token_url = format!(
        "{}/v1/oauth/token",
        api_base.trim_end_matches('/')
    );
    let origin = claude_ai_base.trim_end_matches('/');
    let response = client
        .post(token_url)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .header(USER_AGENT, HeaderValue::from_static(TOKEN_UA))
        .header("accept", HeaderValue::from_static("application/json, text/plain, */*"))
        .header(
            "accept-language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        )
        .header("origin", HeaderValue::from_str(origin).unwrap_or_else(|_| HeaderValue::from_static("https://claude.ai")))
        .header(
            "referer",
            HeaderValue::from_str(&format!("{origin}/"))
                .unwrap_or_else(|_| HeaderValue::from_static("https://claude.ai/")),
        )
        .body(payload)
        .send()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .bytes()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    if !status.is_success() {
        return Err(UpstreamPassthroughError::new(status, headers, body));
    }
    let raw = serde_json::from_slice::<TokenResponseRaw>(&body).map_err(|err| {
        UpstreamPassthroughError::service_unavailable(err.to_string())
    })?;
    let expires_at = raw
        .expires_in
        .map(|seconds| (OffsetDateTime::now_utc().unix_timestamp() + seconds) * 1000);
    let mut scopes = scope
        .split_whitespace()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    if let Some(scope) = raw.scope.as_deref() {
        scopes = scope
            .split_whitespace()
            .map(|value| value.to_string())
            .collect();
    }
    warn!(
        event = "claudecode.oauth_scope",
        scope = %scopes.join(" ")
    );

    let subscription_type = extract_subscription(&raw).map(|value| value.to_string());
    Ok(OAuthTokens {
        access_token: raw.access_token,
        refresh_token: raw.refresh_token,
        expires_at,
        scopes,
        email: None,
        subscription_type,
        rate_limit_tier: None,
        session_key: None,
    })
}

pub(super) async fn enrich_with_profile(
    tokens: &mut OAuthTokens,
    proxy: Option<&str>,
    api_base: &str,
) -> Result<(), UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let mut headers = HeaderMap::new();
    let mut bearer = String::with_capacity(tokens.access_token.len() + 7);
    bearer.push_str("Bearer ");
    bearer.push_str(&tokens.access_token);
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&bearer)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?,
    );
    headers.insert(USER_AGENT, HeaderValue::from_static(CLAUDE_CODE_UA));
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        super::HEADER_BETA,
        HeaderValue::from_static(super::OAUTH_BETA),
    );
    let profile_url = format!(
        "{}/api/oauth/profile",
        api_base.trim_end_matches('/')
    );
    let response = client
        .get(profile_url)
        .headers(headers)
        .send()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(UpstreamPassthroughError::service_unavailable(format!(
            "profile fetch failed: {status}"
        )));
    }
    let body = response
        .bytes()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let payload = serde_json::from_slice::<JsonValue>(&body).map_err(|err| {
        UpstreamPassthroughError::service_unavailable(err.to_string())
    })?;
    let email = payload
        .get("account")
        .and_then(|account| account.get("email"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let has_max = payload
        .get("account")
        .and_then(|account| account.get("has_claude_max"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let has_pro = payload
        .get("account")
        .and_then(|account| account.get("has_claude_pro"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let subscription = if has_max {
        Some("claude_max".to_string())
    } else if has_pro {
        Some("claude_pro".to_string())
    } else {
        None
    };
    let rate_limit_tier = payload
        .get("organization")
        .and_then(|org| org.get("rate_limit_tier"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    if tokens.email.is_none() {
        tokens.email = email;
    }
    if tokens.subscription_type.is_none() {
        tokens.subscription_type = subscription;
    }
    if tokens.rate_limit_tier.is_none() {
        tokens.rate_limit_tier = rate_limit_tier;
    }
    Ok(())
}

pub(super) async fn persist_claudecode_credential(
    pool: &CredentialPool<BaseCredential>,
    provider_id_hint: Option<i64>,
    existing_id: Option<i64>,
    tokens: OAuthTokens,
) -> Result<(), UpstreamPassthroughError> {
    let storage = global_storage().ok_or_else(|| {
        UpstreamPassthroughError::service_unavailable("storage unavailable")
    })?;

    let provider_id = match provider_id_hint {
        Some(id) => id,
        None => {
            let providers = storage
                .list_providers()
                .await
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            providers
                .iter()
                .find(|provider| provider.name == PROVIDER_NAME)
                .map(|provider| provider.id)
                .ok_or_else(|| {
                    UpstreamPassthroughError::service_unavailable("provider not found")
                })?
        }
    };

    let name = tokens
        .email
        .clone()
        .or_else(|| tokens.session_key.as_ref().map(|value| value.chars().take(8).collect()))
        .map(|value| format!("claudecode:{value}"));

    let mut oauth_map = JsonMap::new();
    oauth_map.insert(
        "accessToken".to_string(),
        JsonValue::String(tokens.access_token.clone()),
    );
    if let Some(refresh_token) = tokens.refresh_token.clone() {
        oauth_map.insert("refreshToken".to_string(), JsonValue::String(refresh_token));
    }
    if let Some(expires_at) = tokens.expires_at {
        oauth_map.insert("expiresAt".to_string(), JsonValue::Number(expires_at.into()));
    }
    if !tokens.scopes.is_empty() {
        let scopes = tokens
            .scopes
            .iter()
            .cloned()
            .map(JsonValue::String)
            .collect::<Vec<_>>();
        oauth_map.insert("scopes".to_string(), JsonValue::Array(scopes));
    }
    if let Some(subscription) = tokens.subscription_type.clone() {
        oauth_map.insert("subscriptionType".to_string(), JsonValue::String(subscription));
    }
    if let Some(rate_limit_tier) = tokens.rate_limit_tier.clone() {
        oauth_map.insert("rateLimitTier".to_string(), JsonValue::String(rate_limit_tier));
    }
    let mut secret_map = JsonMap::new();
    secret_map.insert("claudeAiOauth".to_string(), JsonValue::Object(oauth_map));
    if let Some(session_key) = tokens.session_key.clone() {
        secret_map.insert("sessionKey".to_string(), JsonValue::String(session_key));
    }
    let secret = JsonValue::Object(secret_map);

    let mut meta_map = JsonMap::new();
    if let Some(email) = tokens.email.clone() {
        meta_map.insert("email".to_string(), JsonValue::String(email));
    }
    if let Some(plan) = tokens.subscription_type.clone() {
        meta_map.insert("plan".to_string(), JsonValue::String(plan));
    }
    if let Some(rate_limit_tier) = tokens.rate_limit_tier.clone() {
        meta_map.insert("rate_limit_tier".to_string(), JsonValue::String(rate_limit_tier));
    }
    let meta_json = JsonValue::Object(meta_map);

    let input = AdminCredentialInput {
        id: existing_id,
        provider_id,
        name: name.clone(),
        secret: secret.clone(),
        meta_json: meta_json.clone(),
        weight: 100,
        enabled: true,
    };

    storage
        .upsert_credential(input)
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;

    if let Ok(credentials) = storage.list_credentials().await
        && let Some(credential) = credentials.into_iter().find(|credential| {
            if credential.provider_id != provider_id {
                return false;
            }
            if let Some(name) = name.as_ref()
                && credential.name.as_ref() == Some(name) {
                    return true;
                }
            existing_id.map(|id| credential.id == id).unwrap_or(false)
        }) {
            let weight = if credential.weight >= 0 {
                credential.weight as u32
            } else {
                0
            };
            let entry = CredentialEntry::new(
                credential.id.to_string(),
                credential.enabled,
                weight,
                BaseCredential {
                    id: credential.id,
                    name: credential.name.clone(),
                    secret: credential.secret.clone(),
                    meta: credential.meta_json.clone(),
                },
            );
            let snapshot = pool.snapshot();
            let mut credentials = snapshot.credentials.as_ref().clone();
            if let Some(pos) = credentials.iter().position(|item| item.id == entry.id) {
                credentials[pos] = entry;
            } else {
                credentials.push(entry);
            }
            let disallow = snapshot.disallow.as_ref().clone();
            pool.replace_snapshot(PoolSnapshot::new(credentials, disallow));
        }

    Ok(())
}

fn extract_subscription(raw: &TokenResponseRaw) -> Option<&str> {
    raw.account_type
        .as_ref()
        .and_then(|value| value.as_str())
        .or_else(|| raw.plan.as_ref().and_then(|value| value.as_str()))
        .or_else(|| raw.tier.as_ref().and_then(|value| value.as_str()))
        .or_else(|| raw.subscription.as_ref().and_then(|value| value.as_str()))
}

#[allow(clippy::result_large_err)]
fn parse_query<T: for<'de> Deserialize<'de> + Default>(
    query: Option<&str>,
) -> Result<T, UpstreamPassthroughError> {
    match query {
        Some(raw) if !raw.is_empty() => serde_qs::from_str::<T>(raw).map_err(|err| {
            UpstreamPassthroughError::from_status(
                StatusCode::BAD_REQUEST,
                err.to_string(),
            )
        }),
        _ => Ok(T::default()),
    }
}

fn generate_state_and_pkce() -> (String, PkceCodes) {
    let mut state_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut state_bytes);
    let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(state_bytes);
    (state, generate_pkce())
}

fn generate_pkce() -> PkceCodes {
    let mut verifier_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut verifier_bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(verifier_bytes);
        let mut hasher = sha2::Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let digest = hasher.finalize();
        let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

fn build_authorize_url(
    claude_ai_base: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
    scope: &str,
) -> String {
    let redirect_uri = urlencoding::encode(redirect_uri);
    let scope = urlencoding::encode(scope);
    format!(
        "{claude_ai_base}/oauth/authorize?code=true&client_id={CLIENT_ID}&response_type=code&redirect_uri={redirect_uri}&scope={scope}&code_challenge={code_challenge}&code_challenge_method=S256&state={state}"
    )
}

fn oauth_states() -> &'static tokio::sync::RwLock<HashMap<String, OAuthState>> {
    OAUTH_STATES.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
}

fn prune_oauth_states(states: &mut HashMap<String, OAuthState>) {
    let now = OffsetDateTime::now_utc();
    states.retain(|_, state| {
        (now - state.created_at).as_seconds_f64() < OAUTH_STATE_TTL_SECS as f64
    });
}

#[allow(clippy::result_large_err)]
fn json_response(body: JsonValue) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let bytes = serde_json::to_vec(&body)
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(ProxyResponse::Json {
        status: StatusCode::OK,
        headers,
        body: bytes.into(),
    })
}
