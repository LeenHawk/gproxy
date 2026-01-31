use std::collections::HashMap;
use std::sync::OnceLock;

use http::header::CONTENT_TYPE;
use http::HeaderValue;
use serde::{Deserialize, Serialize};

use gproxy_provider_core::{AttemptFailure, DisallowScope, UpstreamContext, UpstreamPassthroughError};

use crate::client::shared_client;
use crate::credential::BaseCredential;

use super::{credential_access_token, credential_refresh_token, invalid_credential};

#[derive(Clone, Debug)]
pub(super) struct CachedTokens {
    pub(super) access_token: String,
}

#[derive(Serialize)]
struct RefreshRequest {
    client_id: &'static str,
    client_secret: &'static str,
    grant_type: &'static str,
    refresh_token: String,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: Option<String>,
}

static TOKEN_CACHE: OnceLock<tokio::sync::RwLock<HashMap<i64, CachedTokens>>> = OnceLock::new();
const REFRESH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "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";

pub(super) async fn ensure_tokens(
    credential: &BaseCredential,
    ctx: &UpstreamContext,
    scope: &DisallowScope,
) -> Result<CachedTokens, AttemptFailure> {
    if let Some(cached) = token_cache().read().await.get(&credential.id).cloned() {
        return Ok(cached);
    }
    if let Some(access_token) = credential_access_token(credential) {
        let tokens = CachedTokens { access_token };
        token_cache().write().await.insert(credential.id, tokens.clone());
        return Ok(tokens);
    }
    if let Some(refresh_token) = credential_refresh_token(credential) {
        return refresh_access_token(credential.id, refresh_token, ctx, scope).await;
    }
    Err(invalid_credential(scope, "missing access_token/refresh_token"))
}

async fn refresh_access_token(
    credential_id: i64,
    refresh_token: String,
    ctx: &UpstreamContext,
    scope: &DisallowScope,
) -> Result<CachedTokens, AttemptFailure> {
    let client = shared_client(ctx.proxy.as_deref())?;
    let request = RefreshRequest {
        client_id: CLIENT_ID,
        client_secret: CLIENT_SECRET,
        grant_type: "refresh_token",
        refresh_token: refresh_token.clone(),
    };
    let response = client
        .post(REFRESH_TOKEN_URL)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
        .form(&request)
        .send()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = format!("refresh_token failed: {status}: {body}");
        let mark = if status == http::StatusCode::UNAUTHORIZED {
            Some(gproxy_provider_core::DisallowMark {
                scope: scope.clone(),
                level: gproxy_provider_core::DisallowLevel::Dead,
                duration: None,
                reason: Some("refresh_token_invalid".to_string()),
            })
        } else {
            None
        };
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(message),
            mark,
        });
    }
    let payload = response.json::<RefreshResponse>().await.map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let access_token = payload.access_token.ok_or_else(|| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(
            "refresh_token response missing access_token".to_string(),
        ),
        mark: None,
    })?;
    let tokens = CachedTokens { access_token };
    token_cache().write().await.insert(credential_id, tokens.clone());
    Ok(tokens)
}

fn token_cache() -> &'static tokio::sync::RwLock<HashMap<i64, CachedTokens>> {
    TOKEN_CACHE.get_or_init(|| tokio::sync::RwLock::new(HashMap::new()))
}
