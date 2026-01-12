use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::Deserialize;

use crate::context::AppContext;
use crate::providers::codex::CodexCredential;

#[derive(Debug, Deserialize)]
pub(crate) struct CodexUsage {
    pub rate_limit: CodexRateLimit,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRateLimit {
    pub allowed: bool,
    pub limit_reached: bool,
    pub primary_window: CodexRateLimitWindow,
    pub secondary_window: Option<CodexRateLimitWindow>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRateLimitWindow {
    pub reset_after_seconds: i64,
    pub reset_at: i64,
}

impl CodexRateLimitWindow {
    pub fn start_ts(&self) -> i64 {
        self.reset_at - self.reset_after_seconds
    }
}

pub(crate) async fn fetch_codex_usage(
    ctx: &AppContext,
    credential: &CodexCredential,
) -> Result<CodexUsage, StatusCode> {
    let setting = ctx
        .codex()
        .get_config()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut url = setting.base_url.clone();
    url.set_path("/backend-api/wham/usage");
    url.set_query(None);

    let mut headers = HeaderMap::new();
    apply_codex_usage_headers(&mut headers, credential.account_id.as_str())?;
    let auth_value = HeaderValue::from_str(&format!("Bearer {}", credential.access_token))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert(header::AUTHORIZATION, auth_value);

    let res = ctx
        .http_client()
        .get(url.as_str())
        .headers(headers)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !res.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_GATEWAY)
}

pub(crate) async fn fetch_codex_usage_by_account(
    ctx: &AppContext,
    account_id: &str,
) -> Result<CodexUsage, StatusCode> {
    let credentials = ctx
        .codex()
        .get_credentials()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let credential = credentials
        .iter()
        .find(|item| item.account_id == account_id && !item.access_token.trim().is_empty())
        .ok_or(StatusCode::NOT_FOUND)?;
    fetch_codex_usage(ctx, credential).await
}

pub(crate) fn cooldown_until_from_usage(usage: &CodexUsage) -> i64 {
    usage
        .rate_limit
        .secondary_window
        .as_ref()
        .map(|window| window.reset_at)
        .unwrap_or(usage.rate_limit.primary_window.reset_at)
}

fn apply_codex_usage_headers(
    headers: &mut HeaderMap,
    account_id: &str,
) -> Result<(), StatusCode> {
    let account_id_value =
        HeaderValue::from_str(account_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert("chatgpt-account-id", account_id_value);
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(header::USER_AGENT, HeaderValue::from_static("codex-cli"));
    Ok(())
}
