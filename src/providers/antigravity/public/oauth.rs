use std::collections::HashSet;
use std::sync::Arc;

use axum::extract::{Extension, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect, Response};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::admin::ensure_admin;
use crate::context::AppContext;
use crate::oauth;
use crate::providers::antigravity::AntigravityCredential;
use crate::providers::antigravity::{apply_usage_to_states, fetch_antigravity_usage};
use crate::providers::antigravity::constants::{
    ANTIGRAVITY_AUTHORIZE_URL, ANTIGRAVITY_CLIENT_ID, ANTIGRAVITY_CLIENT_SECRET,
    ANTIGRAVITY_REDIRECT_URI, ANTIGRAVITY_SCOPE, ANTIGRAVITY_TOKEN_URL, ANTIGRAVITY_USER_AGENT,
};
use crate::providers::google_oauth;
use crate::providers::google_project::fetch_project_id;
use crate::providers::credential_status::{CredentialStatusList, ProviderKind, now_timestamp};
use crate::usage::{DEFAULT_USAGE_VIEWS, next_anchor_ts, set_default_usage_anchors};

const OAUTH_TTL_SECS: i64 = oauth::DEFAULT_TTL_SECS;

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(flatten)]
    pub oauth: oauth::CallbackQuery,
    pub project_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OAuthCallbackResponse {
    pub project_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
struct PendingOauth {
    project_id: Option<String>,
}

static PENDING_OAUTH: oauth::PendingStore<PendingOauth> = oauth::PendingStore::new();

pub(crate) async fn antigravity_oauth_start(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<OAuthStartQuery>,
) -> Result<Response, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let auth_url = antigravity_oauth_authorize_url(query.project_id).await?;
    if wants_json(&headers) {
        Ok(Json(OAuthStartResponse { auth_url }).into_response())
    } else {
        Ok(Redirect::to(&auth_url).into_response())
    }
}

pub(crate) async fn antigravity_oauth_authorize_url(
    project_id: Option<String>,
) -> Result<String, StatusCode> {
    let redirect_uri = ANTIGRAVITY_REDIRECT_URI.to_string();
    let state = oauth::generate_state();
    let code_verifier = oauth::generate_code_verifier();
    let code_challenge = oauth::pkce_challenge(&code_verifier);
    let auth_url = build_authorize_url(&redirect_uri, &state, &code_challenge)?;

    let now = now_timestamp();
    let pending = oauth::PendingEntry::new(
        redirect_uri.clone(),
        code_verifier,
        now,
        PendingOauth { project_id },
    );
    PENDING_OAUTH
        .insert(state.clone(), pending, now, OAUTH_TTL_SECS)
        .await;

    Ok(auth_url)
}

pub(crate) async fn antigravity_oauth_callback(
    Extension(ctx): Extension<Arc<AppContext>>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Result<Json<OAuthCallbackResponse>, StatusCode> {
    if query.oauth.error.is_some() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (code, state) = oauth::extract_callback_params(&query.oauth)?;

    let pending = PENDING_OAUTH
        .take(&state, now_timestamp(), OAUTH_TTL_SECS)
        .await;
    let Some(pending) = pending else {
        return Err(StatusCode::BAD_REQUEST);
    };

    let tokens = exchange_code_for_tokens(
        ctx.as_ref(),
        &pending.redirect_uri,
        &pending.code_verifier,
        &code,
    )
    .await?;
    let mut project_id = pending
        .data
        .project_id
        .or(query.project_id)
        .or(tokens.project_id.clone());
    if project_id.is_none()
        && let Ok(setting) = ctx.antigravity().get_config().await
        && let Ok(found) = fetch_project_id(
            ctx.as_ref(),
            &setting.base_url,
            &tokens.access_token,
            ANTIGRAVITY_USER_AGENT,
        )
        .await
    {
        project_id = found;
    }
    let project_id = match project_id {
        Some(project_id) => project_id,
        None => generate_unique_project_id(ctx.as_ref()).await?,
    };
    let expiry = google_oauth::format_expiry(tokens.expires_in.map(|value| now_timestamp() + value));
    let scope = parse_scope(tokens.scope.as_deref(), ANTIGRAVITY_SCOPE);

    let mut credential = AntigravityCredential {
        project_id: project_id.clone(),
        client_email: String::new(),
        client_id: ANTIGRAVITY_CLIENT_ID.to_string(),
        client_secret: ANTIGRAVITY_CLIENT_SECRET.to_string(),
        token: tokens.access_token,
        refresh_token: tokens.refresh_token.unwrap_or_default(),
        scope,
        token_uri: ANTIGRAVITY_TOKEN_URL.to_string(),
        expiry,
        states: CredentialStatusList::default(),
    };
    if let Ok(usage) = fetch_antigravity_usage(ctx.as_ref(), &credential).await {
        let mut next_states = credential.states.clone();
        if apply_usage_to_states(&mut next_states, &usage) {
            credential.states = next_states;
        }
    }

    let storage = ctx.antigravity();
    let project_key = credential.project_id.clone();
    let exists = storage
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .any(|item| item.project_id == project_key);
    if exists {
        storage
            .update_credential_by_id(&project_key, move |stored| {
                *stored = credential;
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    } else {
        storage
            .add_credential(credential)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let now = now_timestamp();
    let _ = set_default_usage_anchors(
        ctx.usage_store().as_ref(),
        ProviderKind::Antigravity,
        now,
    )
    .await;
    for spec in DEFAULT_USAGE_VIEWS {
        if spec.slot_secs <= 0 {
            continue;
        }
        let next_until = next_anchor_ts(now, spec.slot_secs, now);
        ctx.schedule_usage_anchor(ProviderKind::Antigravity, spec.name, next_until)
            .await;
    }

    Ok(Json(OAuthCallbackResponse {
        project_id,
        status: "ok".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
}

#[derive(Debug, Serialize)]
struct OAuthStartResponse {
    auth_url: String,
}

fn wants_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.contains("application/json"))
        .unwrap_or(false)
}

#[derive(Debug)]
struct ExchangedTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    scope: Option<String>,
    project_id: Option<String>,
}

async fn exchange_code_for_tokens(
    ctx: &AppContext,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
) -> Result<ExchangedTokens, StatusCode> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "authorization_code")
        .append_pair("code", code)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("client_id", ANTIGRAVITY_CLIENT_ID)
        .append_pair("client_secret", ANTIGRAVITY_CLIENT_SECRET)
        .append_pair("code_verifier", code_verifier)
        .finish();

    let res = ctx
        .http_client()
        .post(ANTIGRAVITY_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if !res.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let bytes = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let parsed: TokenResponse = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(ExchangedTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_in: parsed.expires_in,
        scope: parsed.scope,
        project_id: None,
    })
}

fn build_authorize_url(
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, StatusCode> {
    let mut url = Url::parse(ANTIGRAVITY_AUTHORIZE_URL)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("client_id", ANTIGRAVITY_CLIENT_ID);
        pairs.append_pair("redirect_uri", redirect_uri);
        pairs.append_pair("scope", ANTIGRAVITY_SCOPE);
        pairs.append_pair("response_type", "code");
        pairs.append_pair("access_type", "offline");
        pairs.append_pair("prompt", "consent");
        pairs.append_pair("include_granted_scopes", "true");
        pairs.append_pair("code_challenge", code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("state", state);
    }
    Ok(url.to_string())
}

fn parse_scope(scope: Option<&str>, fallback: &str) -> Vec<String> {
    scope
        .unwrap_or(fallback)
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}

async fn generate_unique_project_id(ctx: &AppContext) -> Result<String, StatusCode> {
    let existing = ctx
        .antigravity()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let existing_ids: HashSet<String> = existing
        .into_iter()
        .map(|item| item.project_id)
        .collect();

    loop {
        let candidate = random_project_id();
        if !existing_ids.contains(&candidate) {
            return Ok(candidate);
        }
    }
}

fn random_project_id() -> String {
    let mut bytes = [0u8; 6];
    rand::rng().fill_bytes(&mut bytes);
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        hex.push_str(&format!("{:02x}", byte));
    }
    format!("projects/random-{hex}/locations/global")
}
