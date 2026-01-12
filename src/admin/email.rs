use std::sync::Arc;

use axum::extract::{Extension, Json};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::context::AppContext;
use crate::providers::credential_status::now_timestamp;
use crate::providers::google_oauth;

const GOOGLE_USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

#[derive(Debug, Deserialize)]
pub struct FetchEmailRequest {
    #[serde(default, alias = "project_id", alias = "credential_id", alias = "id")]
    pub project_id: String,
}

fn normalize_project_id(value: &str) -> Result<String, StatusCode> {
    let value = value.trim();
    if value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(value.to_string())
}

async fn fetch_user_email(
    ctx: &AppContext,
    access_token: &str,
) -> Result<Option<String>, StatusCode> {
    let token = access_token.trim();
    if token.is_empty() {
        return Ok(None);
    }

    let mut headers = HeaderMap::new();
    let auth = format!("Bearer {token}");
    let auth_value = HeaderValue::from_str(&auth).map_err(|_| StatusCode::BAD_GATEWAY)?;
    headers.insert(header::AUTHORIZATION, auth_value);

    let res = ctx
        .http_client()
        .get(GOOGLE_USERINFO_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = res.status();
    let body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !status.is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let value: Value = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(value
        .get("email")
        .and_then(|item| item.as_str())
        .map(|email| email.to_string()))
}

#[cfg(feature = "provider-geminicli")]
async fn ensure_geminicli_access_token(
    ctx: &AppContext,
    credential: &crate::providers::geminicli::GeminiCliCredential,
) -> Result<String, StatusCode> {
    let now = now_timestamp();
    if !credential.token.is_empty() && !google_oauth::should_refresh(&credential.expiry, now) {
        return Ok(credential.token.clone());
    }
    if credential.refresh_token.trim().is_empty() {
        if credential.token.is_empty() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return Ok(credential.token.clone());
    }

    let refreshed = google_oauth::refresh_access_token(
        ctx,
        &credential.token_uri,
        &credential.client_id,
        &credential.client_secret,
        &credential.refresh_token,
    )
    .await?;
    let expires_at = refreshed.expires_in.map(|value| now + value);
    let expiry = google_oauth::format_expiry(expires_at);
    let refresh_token = refreshed
        .refresh_token
        .unwrap_or_else(|| credential.refresh_token.clone());
    let scope = refreshed
        .scope
        .map(google_oauth::parse_scope)
        .unwrap_or_else(|| credential.scope.clone());

    let project_id = credential.project_id.clone();
    let access_token = refreshed.access_token.clone();
    let expiry_clone = expiry.clone();
    let refresh_clone = refresh_token.clone();
    let scope_clone = scope.clone();
    ctx.geminicli()
        .update_credential_by_id(&project_id, move |stored| {
            stored.token = access_token.clone();
            stored.refresh_token = refresh_clone.clone();
            if !expiry_clone.is_empty() {
                stored.expiry = expiry_clone.clone();
            }
            if !scope_clone.is_empty() {
                stored.scope = scope_clone.clone();
            }
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(refreshed.access_token)
}

#[cfg(feature = "provider-antigravity")]
async fn ensure_antigravity_access_token(
    ctx: &AppContext,
    credential: &crate::providers::antigravity::AntigravityCredential,
) -> Result<String, StatusCode> {
    let now = now_timestamp();
    if !credential.token.is_empty() && !google_oauth::should_refresh(&credential.expiry, now) {
        return Ok(credential.token.clone());
    }
    if credential.refresh_token.trim().is_empty() {
        if credential.token.is_empty() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
        return Ok(credential.token.clone());
    }

    let refreshed = google_oauth::refresh_access_token(
        ctx,
        &credential.token_uri,
        &credential.client_id,
        &credential.client_secret,
        &credential.refresh_token,
    )
    .await?;
    let expires_at = refreshed.expires_in.map(|value| now + value);
    let expiry = google_oauth::format_expiry(expires_at);
    let refresh_token = refreshed
        .refresh_token
        .unwrap_or_else(|| credential.refresh_token.clone());
    let scope = refreshed
        .scope
        .map(google_oauth::parse_scope)
        .unwrap_or_else(|| credential.scope.clone());

    let project_id = credential.project_id.clone();
    let access_token = refreshed.access_token.clone();
    let expiry_clone = expiry.clone();
    let refresh_clone = refresh_token.clone();
    let scope_clone = scope.clone();
    ctx.antigravity()
        .update_credential_by_id(&project_id, move |stored| {
            stored.token = access_token.clone();
            stored.refresh_token = refresh_clone.clone();
            if !expiry_clone.is_empty() {
                stored.expiry = expiry_clone.clone();
            }
            if !scope_clone.is_empty() {
                stored.scope = scope_clone.clone();
            }
        })
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(refreshed.access_token)
}

#[cfg(feature = "provider-geminicli")]
pub async fn fetch_geminicli_email(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Json(payload): Json<FetchEmailRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let project_id = normalize_project_id(&payload.project_id)?;

    let credentials = ctx
        .geminicli()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credential = credentials
        .iter()
        .find(|item| item.project_id == project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let token = ensure_geminicli_access_token(&ctx, credential).await?;
    let email = fetch_user_email(&ctx, &token).await?;

    if let Some(email) = email {
        let email_clone = email.clone();
        ctx.geminicli()
            .update_credential_by_id(&project_id, move |stored| {
                stored.client_email = email_clone.clone();
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok((
            StatusCode::OK,
            Json(json!({
                "project_id": project_id,
                "user_email": email,
                "message": "user email fetched"
            })),
        ))
    } else {
        Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "project_id": project_id,
                "user_email": null,
                "message": "user email unavailable"
            })),
        ))
    }
}

#[cfg(feature = "provider-antigravity")]
pub async fn fetch_antigravity_email(
    Extension(ctx): Extension<Arc<AppContext>>,
    headers: HeaderMap,
    Json(payload): Json<FetchEmailRequest>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    super::ensure_admin(&headers, &ctx)?;
    let project_id = normalize_project_id(&payload.project_id)?;

    let credentials = ctx
        .antigravity()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credential = credentials
        .iter()
        .find(|item| item.project_id == project_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let token = ensure_antigravity_access_token(&ctx, credential).await?;
    let email = fetch_user_email(&ctx, &token).await?;

    if let Some(email) = email {
        let email_clone = email.clone();
        ctx.antigravity()
            .update_credential_by_id(&project_id, move |stored| {
                stored.client_email = email_clone.clone();
            })
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok((
            StatusCode::OK,
            Json(json!({
                "project_id": project_id,
                "user_email": email,
                "message": "user email fetched"
            })),
        ))
    } else {
        Ok((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "project_id": project_id,
                "user_email": null,
                "message": "user email unavailable"
            })),
        ))
    }
}
