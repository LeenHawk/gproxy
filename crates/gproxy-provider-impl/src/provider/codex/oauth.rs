use std::collections::HashMap;
use std::sync::OnceLock;

use base64::Engine;
use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use rand::RngCore;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use gproxy_provider_core::{
    CredentialEntry, CredentialPool, PoolSnapshot, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_storage::AdminCredentialInput;

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::storage::global_storage;

use super::{headers_to_json, PROVIDER_NAME, CLIENT_ID};

const DEFAULT_ISSUER: &str = "https://auth.openai.com";
const OAUTH_SCOPE: &str = "openid profile email offline_access";
const ORIGINATOR: &str = "codex_vscode";
const OAUTH_STATE_TTL_SECS: i64 = 600;

#[derive(Clone, Debug)]
struct OAuthState {
    code_verifier: String,
    redirect_uri: String,
    created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthStartQuery {
    redirect_uri: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    id_token: String,
}

#[derive(Debug)]
struct PkceCodes {
    code_verifier: String,
    code_challenge: String,
}

#[derive(Debug, Default)]
struct IdTokenClaims {
    email: Option<String>,
    plan: Option<String>,
    account_id: Option<String>,
}

static OAUTH_STATES: OnceLock<tokio::sync::RwLock<HashMap<String, OAuthState>>> = OnceLock::new();

pub(super) async fn handle_oauth_start(
    query: Option<String>,
    headers: HeaderMap,
    ctx: UpstreamContext,
) -> Result<super::UpstreamOk, UpstreamPassthroughError> {
    let params: OAuthStartQuery = parse_query(query.as_deref())?;
    let redirect_uri = match params
        .redirect_uri
        .or_else(|| default_redirect_uri(&headers))
    {
        Some(uri) => uri,
        None => {
            return Err(UpstreamPassthroughError::from_status(
                StatusCode::BAD_REQUEST,
                "missing redirect_uri and unable to infer from Host header",
            ))
        }
    };
    let (state_id, pkce) = generate_state_and_pkce();
    let issuer = oauth_issuer(&ctx).await?;
    let auth_url = build_authorize_url(&issuer, &redirect_uri, &pkce.code_challenge, &state_id);

    let mut guard = oauth_states().write().await;
    prune_oauth_states(&mut guard);
    guard.insert(
        state_id.clone(),
        OAuthState {
            code_verifier: pkce.code_verifier,
            redirect_uri: redirect_uri.clone(),
            created_at: OffsetDateTime::now_utc(),
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
        operation: "codex.oauth".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/codex/oauth".to_string(),
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
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            message,
        ));
    }
    let Some(code) = params.code else {
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            "missing code",
        ));
    };
    let oauth_state = {
        let mut guard = oauth_states().write().await;
        prune_oauth_states(&mut guard);
        match params.state {
            Some(state_id) => guard.remove(&state_id),
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
        return Err(UpstreamPassthroughError::from_status(
            StatusCode::BAD_REQUEST,
            "missing state (multiple or no pending oauth states)",
        ));
    };

    let issuer = oauth_issuer(&ctx).await?;
    let tokens = exchange_code_for_tokens(
        &issuer,
        &oauth_state.redirect_uri,
        &oauth_state.code_verifier,
        &code,
        ctx.proxy.as_deref(),
    )
    .await?;

    let claims = parse_id_token_claims(&tokens.id_token);
    let account_id = claims.account_id.clone();
    let last_refresh = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string());

    persist_codex_credential(
        pool,
        ctx.provider_id,
        &tokens,
        &claims,
    )
    .await?;

    let body = json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "id_token": tokens.id_token,
        "account_id": account_id,
        "email": claims.email,
        "plan": claims.plan,
        "last_refresh": last_refresh,
    });
    let response = json_response(body)?;
    let meta = UpstreamRecordMeta {
        provider: PROVIDER_NAME.to_string(),
        provider_id: ctx.provider_id,
        credential_id: None,
        operation: "codex.oauth_callback".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/codex/oauth/callback".to_string(),
        request_query: query,
        request_headers: headers_to_json(&headers),
        request_body: String::new(),
    };
    Ok(super::UpstreamOk { response, meta })
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
    let mut bytes = [0u8; 64];
    rand::rng().fill_bytes(&mut bytes);
    let code_verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let digest = Sha256::digest(code_verifier.as_bytes());
    let code_challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
    PkceCodes {
        code_verifier,
        code_challenge,
    }
}

fn build_authorize_url(
    issuer: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let params = [
        ("response_type", "code"),
        ("client_id", CLIENT_ID),
        ("redirect_uri", redirect_uri),
        ("scope", OAUTH_SCOPE),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("id_token_add_organizations", "true"),
        ("codex_cli_simplified_flow", "true"),
        ("originator", ORIGINATOR),
        ("state", state),
    ];
    let qs = params
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{}/oauth/authorize?{qs}", issuer.trim_end_matches('/'))
}

fn prune_oauth_states(states: &mut HashMap<String, OAuthState>) {
    let now = OffsetDateTime::now_utc();
    states.retain(|_, entry| (now - entry.created_at).whole_seconds() <= OAUTH_STATE_TTL_SECS);
}

async fn exchange_code_for_tokens(
    issuer: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    proxy: Option<&str>,
) -> Result<TokenResponse, UpstreamPassthroughError> {
    let client = shared_client(proxy).map_err(|err| err.passthrough)?;
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding::encode(code),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(code_verifier),
    );
    let response = client
        .post(format!("{}/oauth/token", issuer.trim_end_matches('/')))
        .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .body(body)
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

async fn oauth_issuer(ctx: &UpstreamContext) -> Result<String, UpstreamPassthroughError> {
    let mut issuer = DEFAULT_ISSUER.to_string();
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
            && let Some(map) = provider.config_json.as_object()
            && let Some(value) = map.get("oauth_issuer").and_then(|v| v.as_str())
        {
            issuer = value.to_string();
        }
    }
    Ok(issuer)
}

fn parse_id_token_claims(id_token: &str) -> IdTokenClaims {
    let mut claims = IdTokenClaims::default();
    let mut parts = id_token.split('.');
    let (_h, payload_b64, _s) = match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s)) if !h.is_empty() && !p.is_empty() && !s.is_empty() => {
            (h, p, s)
        }
        _ => return claims,
    };
    let payload_bytes = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload_b64) {
        Ok(bytes) => bytes,
        Err(_) => return claims,
    };
    let payload = match serde_json::from_slice::<JsonValue>(&payload_bytes) {
        Ok(value) => value,
        Err(_) => return claims,
    };

    let email = payload
        .get("email")
        .and_then(|value| value.as_str())
        .or_else(|| {
            payload
                .get("https://api.openai.com/profile")
                .and_then(|profile| profile.get("email"))
                .and_then(|value| value.as_str())
        })
        .map(|value| value.to_string());

    let (plan, account_id) = payload
        .get("https://api.openai.com/auth")
        .map(|auth| {
            let plan = auth
                .get("chatgpt_plan_type")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            let account_id = auth
                .get("chatgpt_account_id")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string());
            (plan, account_id)
        })
        .unwrap_or((None, None));

    claims.email = email;
    claims.plan = plan;
    claims.account_id = account_id;
    claims
}

fn default_redirect_uri(headers: &HeaderMap) -> Option<String> {
    let _ = headers;
    Some("http://localhost:1455/auth/callback".to_string())
}


fn oauth_states() -> &'static tokio::sync::RwLock<HashMap<String, OAuthState>> {
    OAUTH_STATES.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
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

async fn persist_codex_credential(
    pool: &CredentialPool<BaseCredential>,
    provider_id_hint: Option<i64>,
    tokens: &TokenResponse,
    claims: &IdTokenClaims,
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

    let account_id = claims.account_id.clone();
    let credential_name = claims
        .email
        .clone()
        .or_else(|| account_id.clone())
        .map(|value| format!("codex:{value}"));

    let mut secret_map = serde_json::Map::new();
    secret_map.insert(
        "access_token".to_string(),
        JsonValue::String(tokens.access_token.clone()),
    );
    secret_map.insert(
        "refresh_token".to_string(),
        JsonValue::String(tokens.refresh_token.clone()),
    );
    secret_map.insert(
        "id_token".to_string(),
        JsonValue::String(tokens.id_token.clone()),
    );
    if let Some(account_id) = account_id.clone() {
        secret_map.insert("account_id".to_string(), JsonValue::String(account_id));
    } else {
        secret_map.insert("account_id".to_string(), JsonValue::Null);
    }
    let secret = JsonValue::Object(secret_map);

    let mut meta_map = serde_json::Map::new();
    if let Some(email) = claims.email.clone() {
        meta_map.insert("email".to_string(), JsonValue::String(email));
    }
    if let Some(plan) = claims.plan.clone() {
        meta_map.insert("plan".to_string(), JsonValue::String(plan));
    }
    let meta_json = JsonValue::Object(meta_map);

    let existing_id = storage
        .list_credentials()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?
        .into_iter()
        .find(|credential| {
            if credential.provider_id != provider_id {
                return false;
            }
            if let Some(name) = credential_name.as_ref()
                && credential.name.as_ref() == Some(name) {
                    return true;
                }
            credential
                .secret
                .get("account_id")
                .and_then(|value| value.as_str())
                .and_then(|value| account_id.as_deref().map(|id| id == value))
                .unwrap_or(false)
        })
        .map(|credential| credential.id);

    let input = AdminCredentialInput {
        id: existing_id,
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
            credential
                .secret
                .get("account_id")
                .and_then(|value| value.as_str())
                .and_then(|value| account_id.as_deref().map(|id| id == value))
                .unwrap_or(false)
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
