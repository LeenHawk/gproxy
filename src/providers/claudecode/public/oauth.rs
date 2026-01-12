use std::sync::Arc;

use axum::extract::{Extension, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect, Response};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::admin::ensure_admin;
use crate::context::AppContext;
use crate::oauth;
use crate::providers::claudecode::{
    ClaudeCodeCredential, CLAUDE_AI_AUTHORIZE_URL, CLAUDE_API_VERSION,
    CLAUDE_BETA_BASE, CLAUDE_CODE_CLIENT_ID, CLAUDE_CODE_REDIRECT_URI,
    CLAUDE_CODE_SCOPE,
};
use crate::providers::credential_status::{CredentialStatusList, ProviderKind, now_timestamp};
use crate::usage::{DEFAULT_USAGE_VIEWS, next_anchor_ts, set_default_usage_anchors};

const OAUTH_TTL_SECS: i64 = oauth::DEFAULT_TTL_SECS;

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(flatten)]
    pub oauth: oauth::CallbackQuery,
}

#[derive(Debug, Serialize)]
pub struct OAuthCallbackResponse {
    pub status: String,
}

static PENDING_OAUTH: oauth::PendingStore<()> = oauth::PendingStore::new();

pub(crate) async fn claudecode_oauth_start(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(_query): Query<OAuthStartQuery>,
) -> Result<Response, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let auth_url = claudecode_oauth_authorize_url().await?;
    if wants_json(&headers) {
        Ok(Json(OAuthStartResponse { auth_url }).into_response())
    } else {
        Ok(Redirect::to(&auth_url).into_response())
    }
}

pub(crate) async fn claudecode_oauth_authorize_url() -> Result<String, StatusCode> {
    let redirect_uri = CLAUDE_CODE_REDIRECT_URI.to_string();
    let state = oauth::generate_state();
    let code_verifier = oauth::generate_code_verifier();
    let code_challenge = oauth::pkce_challenge(&code_verifier);
    let auth_url = build_authorize_url(&redirect_uri, &state, &code_challenge)?;

    let now = now_timestamp();
    let pending = oauth::PendingEntry::new(redirect_uri.clone(), code_verifier, now, ());
    PENDING_OAUTH
        .insert(state.clone(), pending, now, OAUTH_TTL_SECS)
        .await;

    Ok(auth_url)
}

pub(crate) async fn claudecode_oauth_callback(
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
        Some(state.as_str()),
    )
    .await?;
    let expires_at = now_timestamp() + tokens.expires_in.unwrap_or(0);

    let credential = ClaudeCodeCredential {
        session_key: String::new(),
        refresh_token: tokens.refresh_token,
        access_token: tokens.access_token,
        expires_at,
        states: CredentialStatusList::default(),
    };

    let storage = ctx.claudecode();
    let refresh_token_key = credential.refresh_token.clone();
    let exists = storage
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .any(|item| item.refresh_token == refresh_token_key);
    if exists {
        storage
            .update_credential_by_id(&refresh_token_key, move |stored| {
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
        ProviderKind::ClaudeCode,
        now,
    )
    .await;
    for spec in DEFAULT_USAGE_VIEWS {
        if spec.slot_secs <= 0 {
            continue;
        }
        let next_until = next_anchor_ts(now, spec.slot_secs, now);
        ctx.schedule_usage_anchor(ProviderKind::ClaudeCode, spec.name, next_until)
            .await;
    }

    Ok(Json(OAuthCallbackResponse {
        status: "ok".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: Option<i64>,
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

async fn exchange_code_for_tokens(
    ctx: &AppContext,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    state: Option<&str>,
) -> Result<TokenResponse, StatusCode> {
    let body = {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        form.append_pair("grant_type", "authorization_code");
        form.append_pair("client_id", CLAUDE_CODE_CLIENT_ID);
        form.append_pair("code", code);
        form.append_pair("code_verifier", code_verifier);
        form.append_pair("redirect_uri", redirect_uri);
        if let Some(state) = state {
            form.append_pair("state", state);
        }
        form.finish()
    };

    let base_url = ctx
        .claudecode()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .base_url;
    let url = base_url
        .join("v1/oauth/token")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let res = ctx
        .http_client()
        .post(url.as_str())
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("anthropic-version", CLAUDE_API_VERSION)
        .header("anthropic-beta", CLAUDE_BETA_BASE)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    if !res.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let bytes = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let parsed: TokenResponse = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(parsed)
}

fn build_authorize_url(
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, StatusCode> {
    let mut url = Url::parse(CLAUDE_AI_AUTHORIZE_URL)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("code", "true");
        pairs.append_pair("client_id", CLAUDE_CODE_CLIENT_ID);
        pairs.append_pair("response_type", "code");
        pairs.append_pair("redirect_uri", redirect_uri);
        pairs.append_pair("scope", CLAUDE_CODE_SCOPE);
        pairs.append_pair("code_challenge", code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("state", state);
    }
    Ok(url.to_string())
}
