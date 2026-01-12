use std::sync::Arc;

use axum::extract::{Extension, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Json, Redirect, Response};
use base64::Engine;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::admin::ensure_admin;
use crate::context::AppContext;
use crate::oauth;
use crate::providers::codex::{CodexCredential, cooldown_until_from_usage, fetch_codex_usage};
use crate::providers::credential_status::{CredentialStatus, CredentialStatusList, ProviderKind, now_timestamp};
use crate::usage::{next_anchor_ts, slot_secs_for_view};

const OAUTH_ISSUER: &str = "https://auth.openai.com";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OAUTH_TTL_SECS: i64 = oauth::DEFAULT_TTL_SECS;

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub workspace_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(flatten)]
    pub oauth: oauth::CallbackQuery,
}

#[derive(Debug, Serialize)]
pub struct OAuthCallbackResponse {
    pub account_id: String,
    pub status: String,
}

#[derive(Debug, Clone)]
struct PendingOauth {
    allowed_workspace_id: Option<String>,
}

static PENDING_OAUTH: oauth::PendingStore<PendingOauth> = oauth::PendingStore::new();

pub(crate) async fn codex_oauth_start(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Query(query): Query<OAuthStartQuery>,
) -> Result<Response, StatusCode> {
    ensure_admin(&headers, &ctx)?;
    let auth_url = codex_oauth_authorize_url(query.workspace_id).await?;
    if wants_json(&headers) {
        Ok(Json(OAuthStartResponse { auth_url }).into_response())
    } else {
        Ok(Redirect::to(&auth_url).into_response())
    }
}

pub(crate) async fn codex_oauth_authorize_url(
    workspace_id: Option<String>,
) -> Result<String, StatusCode> {
    let redirect_uri = resolve_redirect_uri();
    let state = oauth::generate_state();
    let code_verifier = oauth::generate_code_verifier();
    let code_challenge = oauth::pkce_challenge(&code_verifier);
    let auth_url = build_authorize_url(
        redirect_uri.as_str(),
        &state,
        &code_challenge,
        workspace_id.as_deref(),
    )?;

    let now = now_timestamp();
    let pending = oauth::PendingEntry::new(
        redirect_uri.clone(),
        code_verifier,
        now,
        PendingOauth {
            allowed_workspace_id: workspace_id,
        },
    );
    PENDING_OAUTH
        .insert(state.clone(), pending, now, OAUTH_TTL_SECS)
        .await;

    Ok(auth_url)
}

pub(crate) async fn codex_oauth_callback(
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
    let account_id = extract_account_id(&tokens.id_token)
        .ok_or(StatusCode::BAD_GATEWAY)?;
    if let Some(expected) = pending.data.allowed_workspace_id.as_deref()
        && expected != account_id
    {
        return Err(StatusCode::FORBIDDEN);
    }

    let mut credential = CodexCredential {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        account_id: account_id.clone(),
        last_refresh: now_timestamp(),
        states: CredentialStatusList::default(),
    };

    if let Ok(usage) = fetch_codex_usage(ctx.as_ref(), &credential).await {
        let primary_start = usage.rate_limit.primary_window.start_ts();
        let secondary_start = usage
            .rate_limit
            .secondary_window
            .as_ref()
            .map(|window| window.start_ts())
            .unwrap_or(primary_start);
        ctx
            .usage_store()
            .set_anchor(ProviderKind::Codex, "5h", primary_start)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        ctx
            .usage_store()
            .set_anchor(ProviderKind::Codex, "1w", secondary_start)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let now = now_timestamp();
        for (view_name, anchor_ts) in [("5h", primary_start), ("1w", secondary_start)] {
            if let Some(slot_secs) = slot_secs_for_view(ProviderKind::Codex, view_name)
                && slot_secs > 0
            {
                let next_until = next_anchor_ts(anchor_ts, slot_secs, now);
                ctx.schedule_usage_anchor(ProviderKind::Codex, view_name, next_until)
                    .await;
            }
        }
        if usage.rate_limit.limit_reached || !usage.rate_limit.allowed {
            let until = cooldown_until_from_usage(&usage);
            credential.states.update_status(
                crate::providers::credential_status::DEFAULT_MODEL_KEY,
                CredentialStatus::Cooldown { until },
            );
        }
    }

    let storage = ctx.codex();
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

    Ok(Json(OAuthCallbackResponse {
        account_id,
        status: "ok".to_string(),
    }))
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: String,
    access_token: String,
    refresh_token: String,
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

struct ExchangedTokens {
    id_token: String,
    access_token: String,
    refresh_token: String,
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
        .append_pair("client_id", CLIENT_ID)
        .append_pair("code_verifier", code_verifier)
        .finish();

    let res = ctx
        .http_client()
        .post(format!("{OAUTH_ISSUER}/oauth/token"))
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
        id_token: parsed.id_token,
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
    })
}

fn extract_account_id(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let auth = value.get("https://api.openai.com/auth")?;
    let account_id = auth.get("chatgpt_account_id")?.as_str()?;
    Some(account_id.to_string())
}

fn build_authorize_url(
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
    workspace_id: Option<&str>,
) -> Result<String, StatusCode> {
    let mut url = Url::parse(&format!("{OAUTH_ISSUER}/oauth/authorize"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("response_type", "code");
        pairs.append_pair("client_id", CLIENT_ID);
        pairs.append_pair("redirect_uri", redirect_uri);
        pairs.append_pair("scope", "openid profile email offline_access");
        pairs.append_pair("code_challenge", code_challenge);
        pairs.append_pair("code_challenge_method", "S256");
        pairs.append_pair("id_token_add_organizations", "true");
        pairs.append_pair("codex_cli_simplified_flow", "true");
        pairs.append_pair("state", state);
        if let Some(workspace_id) = workspace_id {
            pairs.append_pair("allowed_workspace_id", workspace_id);
        }
    }
    Ok(url.to_string())
}

fn resolve_redirect_uri() -> String {
    "http://localhost:1455/auth/callback".to_string()
}
