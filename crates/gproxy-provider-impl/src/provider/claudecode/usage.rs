use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::Value as JsonValue;

use gproxy_provider_core::{
    AttemptFailure, CredentialEntry, CredentialPool, DisallowScope, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::UpstreamOk;
use crate::upstream::{classify_status, send_with_logging};

use tracing::warn;

use super::{
    channel_urls, credential_refresh_token, credential_session_key, CLAUDE_CODE_UA, OAUTH_BETA,
    PROVIDER_NAME,
};
use super::oauth;
use super::refresh;

struct UsageFetch {
    payload: JsonValue,
    credential_id: i64,
}

pub(super) async fn handle_usage(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<UpstreamOk, UpstreamPassthroughError> {
    let result = fetch_usage_payload_with_credential(pool, ctx.clone()).await?;
    let body_bytes = serde_json::to_vec(&result.payload)
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let response = ProxyResponse::Json {
        status: StatusCode::OK,
        headers: headers.clone(),
        body: body_bytes.into(),
    };
    let meta = UpstreamRecordMeta {
        provider: PROVIDER_NAME.to_string(),
        provider_id: ctx.provider_id,
        credential_id: Some(result.credential_id),
        operation: "claudecode.usage".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/claudecode/usage".to_string(),
        request_query: None,
        request_headers: "{}".to_string(),
        request_body: String::new(),
    };
    Ok(UpstreamOk { response, meta })
}

pub(super) async fn fetch_usage_payload_for_credential(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
    credential_id: i64,
) -> Result<JsonValue, UpstreamPassthroughError> {
    let id = credential_id.to_string();
    let result = fetch_usage_payload_with_credential_id(pool, ctx, &id).await?;
    Ok(result.payload)
}

async fn fetch_usage_payload_with_credential(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<UsageFetch, UpstreamPassthroughError> {
    fetch_usage_payload_with(pool, ctx, None).await
}

async fn fetch_usage_payload_with_credential_id(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
    credential_id: &str,
) -> Result<UsageFetch, UpstreamPassthroughError> {
    fetch_usage_payload_with(pool, ctx, Some(credential_id)).await
}

async fn fetch_usage_payload_with(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
    credential_id: Option<&str>,
) -> Result<UsageFetch, UpstreamPassthroughError> {
    let scope = DisallowScope::AllModels;
    let runner = |credential: CredentialEntry<BaseCredential>| {
        let ctx = ctx.clone();
        let scope = scope.clone();
        async move {
            let tokens = refresh::ensure_tokens(pool, credential.value(), &ctx, &scope).await?;
            let mut access_token = tokens.access_token.clone();
            let refresh_token = tokens
                .refresh_token
                .clone()
                .or_else(|| credential_refresh_token(credential.value()));
            let client = shared_client(ctx.proxy.as_deref())?;
            let channel = channel_urls(&ctx)
                .await
                .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;
            let usage_url = format!("{}/api/oauth/usage", channel.api_base);
            let mut req_headers = build_usage_headers(&access_token)?;
            let mut response = send_with_logging(
                &ctx,
                PROVIDER_NAME,
                "claudecode.usage",
                "GET",
                "/api/oauth/usage",
                None,
                false,
                &scope,
                || client.get(usage_url.clone()).headers(req_headers.clone()).send(),
            )
            .await?;
            let mut status = response.status();
            let mut headers = response.headers().clone();
            let mut body = response
                .bytes()
                .await
                .map_err(|err| crate::upstream::network_failure(err, &scope))?;

            if status == StatusCode::FORBIDDEN && is_scope_error(&body) {
                if let Some(session_key) = credential_session_key(credential.value()) {
                    warn!(
                        event = "claudecode.usage_scope_retry",
                        status = %status.as_u16(),
                        body = %String::from_utf8_lossy(&body)
                    );
                    let mut tokens =
                        refresh::oauth_with_session_key(&session_key, &ctx, &channel).await?;
                    tokens.session_key = Some(session_key.clone());
                    let existing_id = Some(credential.value().id);
                    oauth::persist_claudecode_credential(
                        pool,
                        ctx.provider_id,
                        existing_id,
                        tokens.clone(),
                    )
                    .await
                    .map_err(|err| AttemptFailure {
                        passthrough: err,
                        mark: None,
                    })?;
                    let cached = refresh::CachedTokens {
                        access_token: tokens.access_token,
                        refresh_token: tokens.refresh_token,
                        expires_at: tokens.expires_at,
                    };
                    refresh::cache_tokens(credential.value().id, cached.clone()).await;
                    access_token = cached.access_token;
                    req_headers = build_usage_headers(&access_token)?;
                    response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claudecode.usage",
                        "GET",
                        "/api/oauth/usage",
                        None,
                        false,
                        &scope,
                        || client.get(usage_url.clone()).headers(req_headers.clone()).send(),
                    )
                    .await?;
                    status = response.status();
                    headers = response.headers().clone();
                    body = response
                        .bytes()
                        .await
                        .map_err(|err| crate::upstream::network_failure(err, &scope))?;
                }
            } else if (status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN)
                && let Some(refresh_token) = refresh_token {
                    let refreshed =
                        refresh::refresh_access_token(
                            credential.value().id,
                            refresh_token,
                            &ctx,
                            &scope,
                            &channel,
                        )
                            .await?;
                    access_token = refreshed.access_token;
                    req_headers = build_usage_headers(&access_token)?;
                    response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claudecode.usage",
                        "GET",
                        "/api/oauth/usage",
                        None,
                        false,
                        &scope,
                        || client.get(usage_url.clone()).headers(req_headers.clone()).send(),
                    )
                    .await?;
                    status = response.status();
                    headers = response.headers().clone();
                    body = response
                        .bytes()
                        .await
                        .map_err(|err| crate::upstream::network_failure(err, &scope))?;
                }

            if !status.is_success() {
                let mark = classify_status(status, &headers, &scope);
                return Err(AttemptFailure {
                    passthrough: UpstreamPassthroughError::new(status, headers, body),
                    mark,
                });
            }
            let payload = serde_json::from_slice::<JsonValue>(&body).map_err(|err| {
                AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                }
            })?;
            Ok(UsageFetch {
                payload,
                credential_id: credential.value().id,
            })
        }
    };

    match credential_id {
        Some(id) => pool.execute_for_id(id, scope.clone(), runner).await,
        None => pool.execute(scope.clone(), runner).await,
    }
}

#[allow(clippy::result_large_err)]
fn build_usage_headers(access_token: &str) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    let mut bearer = String::with_capacity(access_token.len() + 7);
    bearer.push_str("Bearer ");
    bearer.push_str(access_token);
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&bearer).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    headers.insert(
        http::header::ACCEPT,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static(CLAUDE_CODE_UA),
    );
    headers.insert(
        super::HEADER_BETA,
        HeaderValue::from_static(OAUTH_BETA),
    );
    Ok(headers)
}

fn is_scope_error(body: &[u8]) -> bool {
    let Ok(payload) = serde_json::from_slice::<JsonValue>(body) else {
        return false;
    };
    let error = payload.get("error");
    let Some(error) = error else {
        return false;
    };
    let error_type = error.get("type").and_then(|value| value.as_str());
    let message = error.get("message").and_then(|value| value.as_str());
    matches!(error_type, Some("permission_error"))
        && message
            .map(|text| text.contains("scope requirement"))
            .unwrap_or(false)
}
