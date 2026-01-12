use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use serde::Deserialize;

use crate::context::AppContext;

const TOKEN_SKEW_SECS: i64 = 30;

#[derive(Debug, Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    scope: Option<String>,
}

#[derive(Debug)]
pub(crate) struct RefreshedToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub scope: Option<String>,
}

pub(crate) async fn refresh_access_token(
    ctx: &AppContext,
    token_uri: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<RefreshedToken, StatusCode> {
    if refresh_token.trim().is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "refresh_token")
        .append_pair("refresh_token", refresh_token)
        .append_pair("client_id", client_id)
        .append_pair("client_secret", client_secret)
        .finish();

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );

    let res = ctx
        .http_client()
        .post(token_uri)
        .headers(headers)
        .body(body)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = res.status();
    let body = res.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !status.is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let parsed: RefreshTokenResponse =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_GATEWAY)?;
    Ok(RefreshedToken {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        expires_in: parsed.expires_in,
        scope: parsed.scope,
    })
}

pub(crate) fn should_refresh(expiry: &str, now: i64) -> bool {
    let expiry = expiry.trim();
    if expiry.is_empty() {
        return true;
    }
    let Ok(dt) = time::OffsetDateTime::parse(
        expiry,
        &time::format_description::well_known::Rfc3339,
    ) else {
        return true;
    };
    dt.unix_timestamp() <= now + TOKEN_SKEW_SECS
}

pub(crate) fn format_expiry(until: Option<i64>) -> String {
    let Some(until) = until else {
        return String::new();
    };
    let Ok(dt) = time::OffsetDateTime::from_unix_timestamp(until) else {
        return String::new();
    };
    dt.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

pub(crate) fn parse_scope(scope: String) -> Vec<String> {
    scope
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .collect()
}
