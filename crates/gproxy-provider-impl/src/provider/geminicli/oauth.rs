use std::collections::HashMap;
use std::sync::OnceLock;

use base64::Engine;
use http::header::{ACCEPT_ENCODING, AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use http::{HeaderMap, HeaderValue, StatusCode};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use serde_json::{json, Value as JsonValue};
use time::OffsetDateTime;
use tokio::time::{Duration, sleep};

use gproxy_provider_core::{
    CredentialEntry, CredentialPool, PoolSnapshot, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_storage::AdminCredentialInput;

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::storage::global_storage;

use super::{GEMINICLI_USER_AGENT, PROVIDER_NAME};

const DEFAULT_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/auth";
const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";
const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
const OAUTH_STATE_TTL_SECS: i64 = 600;

#[derive(Clone, Debug)]
struct OAuthState {
    redirect_uri: String,
    created_at: OffsetDateTime,
    base_url: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthStartQuery {
    redirect_uri: Option<String>,
    base_url: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    redirect_uri: Option<String>,
    base_url: Option<String>,
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

#[derive(Serialize)]
struct TokenRequest<'a> {
    code: &'a str,
    client_id: &'static str,
    client_secret: &'static str,
    redirect_uri: &'a str,
    grant_type: &'static str,
}

static OAUTH_STATES: OnceLock<tokio::sync::RwLock<HashMap<String, OAuthState>>> = OnceLock::new();

pub(super) async fn handle_oauth_start(
    query: Option<String>,
    _headers: HeaderMap,
    ctx: UpstreamContext,
) -> Result<super::UpstreamOk, UpstreamPassthroughError> {
    let params: OAuthStartQuery = parse_query(query.as_deref())?;
    let redirect_uri = params
        .redirect_uri
        .unwrap_or_else(default_redirect_uri);
    let state_id = generate_state();
    let (auth_url, _) = oauth_endpoints(&ctx).await?;
    let auth_url = build_authorize_url(&auth_url, &redirect_uri, &state_id);

    let mut guard = oauth_states().write().await;
    prune_oauth_states(&mut guard);
    guard.insert(
        state_id.clone(),
        OAuthState {
            redirect_uri: redirect_uri.clone(),
            created_at: OffsetDateTime::now_utc(),
            base_url: params.base_url,
            project_id: params.project_id,
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
        operation: "geminicli.oauth".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/geminicli/oauth".to_string(),
        request_query: query,
        request_headers: String::new(),
        request_body: String::new(),
    };
    Ok(super::UpstreamOk { response, meta })
}

pub(super) async fn handle_oauth_callback(
    pool: &CredentialPool<BaseCredential>,
    query: Option<String>,
    _headers: HeaderMap,
    ctx: UpstreamContext,
) -> Result<super::UpstreamOk, UpstreamPassthroughError> {
    let params: OAuthCallbackQuery = parse_query(query.as_deref())?;
    if let Some(error) = params.error.as_deref() {
        let detail = params
            .error_description
            .as_deref()
            .unwrap_or("oauth error");
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            format!("{error}: {detail}"),
        ));
    }
    let code = params.code.ok_or_else(|| {
        UpstreamPassthroughError::from_status(StatusCode::BAD_REQUEST, "missing code")
    })?;

    let (redirect_uri, _base_url, project_id) = if let Some(state) = params.state.as_deref() {
        let mut guard = oauth_states().write().await;
        prune_oauth_states(&mut guard);
        if let Some(state) = guard.remove(state) {
            (
                state.redirect_uri,
                state.base_url,
                state.project_id,
            )
        } else {
            (
                params
                    .redirect_uri
                    .unwrap_or_else(default_redirect_uri),
                params.base_url,
                params.project_id,
            )
        }
    } else {
        (
            params
                .redirect_uri
                .unwrap_or_else(default_redirect_uri),
            params.base_url,
            params.project_id,
        )
    };

    let (_, token_url) = oauth_endpoints(&ctx).await?;
    let tokens =
        exchange_code_for_tokens(&code, &redirect_uri, &token_url, ctx.proxy.as_deref()).await?;
    let base_url = super::channel_base_url(&ctx).await?;
    let project_id = match project_id {
        Some(value) => value,
        None => detect_project_id(
            &tokens.access_token,
            &base_url,
            GEMINICLI_USER_AGENT,
            ctx.proxy.as_deref(),
        )
        .await?
        .ok_or_else(|| {
            UpstreamPassthroughError::from_status(
                StatusCode::BAD_REQUEST,
                "missing project_id (auto-detect failed)",
            )
        })?,
    };

    persist_credential(pool, ctx.provider_id, &tokens, &project_id).await?;
    let body = json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "project_id": project_id,
    });
    let response = json_response(body)?;
    let meta = UpstreamRecordMeta {
        provider: PROVIDER_NAME.to_string(),
        provider_id: ctx.provider_id,
        credential_id: None,
        operation: "geminicli.oauth.callback".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/geminicli/oauth/callback".to_string(),
        request_query: query,
        request_headers: String::new(),
        request_body: String::new(),
    };
    Ok(super::UpstreamOk { response, meta })
}

async fn detect_project_id(
    access_token: &str,
    base_url: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<Option<String>, UpstreamPassthroughError> {
    if let Ok(Some(project_id)) = try_load_code_assist(access_token, base_url, user_agent, proxy).await {
        return Ok(Some(project_id));
    }
    try_onboard_user(access_token, base_url, user_agent, proxy).await
}

async fn try_load_code_assist(
    access_token: &str,
    base_url: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<Option<String>, UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let url = format!("{}/v1internal:loadCodeAssist", base_url.trim_end_matches('/'));
    let mut headers = build_project_headers(access_token, user_agent)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    let response = client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    if !status.is_success() {
        return Err(UpstreamPassthroughError::service_unavailable(format!(
            "loadCodeAssist failed: {status}"
        )));
    }
    let payload = serde_json::from_slice::<JsonValue>(&body)
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let current_tier = payload.get("currentTier");
    if current_tier.is_none() || current_tier.is_some_and(|value| value.is_null()) {
        return Ok(None);
    }
    let project_id = payload
        .get("cloudaicompanionProject")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    Ok(project_id)
}

async fn try_onboard_user(
    access_token: &str,
    base_url: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<Option<String>, UpstreamPassthroughError> {
    let tier_id = get_onboard_tier(access_token, base_url, user_agent, proxy).await?;
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let url = format!("{}/v1internal:onboardUser", base_url.trim_end_matches('/'));
    let mut headers = build_project_headers(access_token, user_agent)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = json!({
        "tierId": tier_id,
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    for _ in 0..5 {
        let response = client
            .post(url.clone())
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        if !status.is_success() {
            return Err(UpstreamPassthroughError::service_unavailable(format!(
                "onboardUser failed: {status}"
            )));
        }
        let payload = serde_json::from_slice::<JsonValue>(&body)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        if payload.get("done").and_then(|value| value.as_bool()) == Some(true) {
            let project_value = payload
                .get("response")
                .and_then(|value| value.get("cloudaicompanionProject"));
            let project_id = project_value
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
                .or_else(|| {
                    project_value
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                });
            return Ok(project_id);
        }
        sleep(Duration::from_secs(2)).await;
    }
    Ok(None)
}

async fn get_onboard_tier(
    access_token: &str,
    base_url: &str,
    user_agent: &str,
    proxy: Option<&str>,
) -> Result<String, UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let url = format!("{}/v1internal:loadCodeAssist", base_url.trim_end_matches('/'));
    let mut headers = build_project_headers(access_token, user_agent)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = json!({
        "metadata": {
            "ideType": "ANTIGRAVITY",
            "platform": "PLATFORM_UNSPECIFIED",
            "pluginType": "GEMINI"
        }
    });
    let response = client
        .post(url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let status = response.status();
    let body = response
        .bytes()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    if !status.is_success() {
        return Ok("LEGACY".to_string());
    }
    let payload = serde_json::from_slice::<JsonValue>(&body)
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let tiers = payload
        .get("allowedTiers")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    for tier in tiers {
        if tier.get("isDefault").and_then(|value| value.as_bool()) == Some(true)
            && let Some(id) = tier.get("id").and_then(|value| value.as_str()) {
                return Ok(id.to_string());
            }
    }
    Ok("LEGACY".to_string())
}
#[warn(clippy::result_large_err)]
fn build_project_headers(
    access_token: &str,
    user_agent: &str,
) -> Result<HeaderMap, UpstreamPassthroughError> {
    let mut headers = HeaderMap::new();
    let mut bearer = String::with_capacity(access_token.len() + 7);
    bearer.push_str("Bearer ");
    bearer.push_str(access_token);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&bearer)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?,
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(user_agent)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?,
    );
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip"));
    Ok(headers)
}

async fn exchange_code_for_tokens(
    code: &str,
    redirect_uri: &str,
    token_url: &str,
    proxy: Option<&str>,
) -> Result<TokenResponse, UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let request = TokenRequest {
        code,
        client_id: CLIENT_ID,
        client_secret: CLIENT_SECRET,
        redirect_uri,
        grant_type: "authorization_code",
    };
    let response = client
        .post(token_url)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .form(&request)
        .send()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let headers = response.headers().clone();
        let body = response
            .bytes()
            .await
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        return Err(UpstreamPassthroughError::new(status, headers, body));
    }
    response
        .json::<TokenResponse>()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))
}

async fn oauth_endpoints(
    ctx: &UpstreamContext,
) -> Result<(String, String), UpstreamPassthroughError> {
    let mut auth_url = DEFAULT_AUTH_URL.to_string();
    let mut token_url = DEFAULT_TOKEN_URL.to_string();
    if let Some(storage) = global_storage() {
        let providers = storage
            .list_providers()
            .await
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let provider = if let Some(id) = ctx.provider_id {
            providers.iter().find(|provider| provider.id == id)
        } else {
            providers.iter().find(|provider| provider.name == PROVIDER_NAME)
        };
        if let Some(provider) = provider
            && let Some(map) = provider.config_json.as_object() {
                if let Some(value) = map.get("oauth_auth_url").and_then(|v| v.as_str()) {
                    auth_url = value.to_string();
                }
                if let Some(value) = map.get("oauth_token_url").and_then(|v| v.as_str()) {
                    token_url = value.to_string();
                }
            }
    }
    Ok((auth_url, token_url))
}

fn build_authorize_url(auth_url: &str, redirect_uri: &str, state: &str) -> String {
    let scope = urlencoding::encode(OAUTH_SCOPE);
    let redirect_uri = urlencoding::encode(redirect_uri);
    format!(
        "{}?response_type=code&client_id={CLIENT_ID}&redirect_uri={redirect_uri}&scope={scope}&access_type=offline&prompt=consent&include_granted_scopes=true&state={state}",
        auth_url.trim_end_matches('/')
    )
}

fn generate_state() -> String {
    let mut bytes = [0u8; 32];
    let mut rng = rand::rng();
    rng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

#[allow(clippy::result_large_err)]
fn parse_query<T: DeserializeOwned>(query: Option<&str>) -> Result<T, UpstreamPassthroughError> {
    let query = query.unwrap_or_default();
    serde_qs::from_str(query).map_err(|err| {
        UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            format!("invalid query: {err}"),
        )
    })
}

fn default_redirect_uri() -> String {
    "http://localhost:1455/auth/callback".to_string()
}

fn oauth_states() -> &'static tokio::sync::RwLock<HashMap<String, OAuthState>> {
    OAUTH_STATES.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
}

fn prune_oauth_states(states: &mut HashMap<String, OAuthState>) {
    let now = OffsetDateTime::now_utc();
    states.retain(|_, state| (now - state.created_at).whole_seconds() < OAUTH_STATE_TTL_SECS);
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

async fn persist_credential(
    pool: &CredentialPool<BaseCredential>,
    provider_id_hint: Option<i64>,
    tokens: &TokenResponse,
    project_id: &str,
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

    let email = tokens
        .id_token
        .as_deref()
        .and_then(parse_id_token_email);
    let credential_name = email
        .as_ref()
        .map(|value| format!("geminicli:{value}"))
        .or_else(|| Some(format!("geminicli:{project_id}")));

    let secret = json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "project_id": project_id,
    });
    let meta_json = json!({});

    let input = AdminCredentialInput {
        id: None,
        provider_id,
        name: credential_name.clone(),
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
            if let Some(name) = credential_name.as_ref()
                && credential.name.as_ref() == Some(name) {
                    return true;
                }
            false
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
            let mut creds = snapshot.credentials.as_ref().clone();
            if let Some(pos) = creds.iter().position(|item| item.id == entry.id) {
                creds[pos] = entry;
            } else {
                creds.push(entry);
            }
            let disallow = snapshot.disallow.as_ref().clone();
            pool.replace_snapshot(PoolSnapshot::new(creds, disallow));
        }

    Ok(())
}

fn parse_id_token_email(id_token: &str) -> Option<String> {
    let mut parts = id_token.split('.');
    let (_h, payload_b64, _s) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => {
            (h, p, s)
        }
        _ => return None,
    };
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload_b64)
        .ok()?;
    let payload = serde_json::from_slice::<JsonValue>(&payload_bytes).ok()?;
    payload
        .get("email")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}
