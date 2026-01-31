use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::Value as JsonValue;

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::UpstreamOk;
use crate::upstream::{classify_status, send_with_logging};

use super::{credential_refresh_token, PROVIDER_NAME, USAGE_URL, CLAUDE_CODE_UA, OAUTH_BETA};
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

async fn fetch_usage_payload_with_credential(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<UsageFetch, UpstreamPassthroughError> {
    let scope = DisallowScope::AllModels;
    pool.execute(scope.clone(), |credential| {
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
                || client.get(USAGE_URL).headers(req_headers.clone()).send(),
            )
            .await?;
            if (response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::FORBIDDEN)
                && let Some(refresh_token) = refresh_token {
                    let refreshed =
                        refresh::refresh_access_token(credential.value().id, refresh_token, &ctx, &scope)
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
                        || client.get(USAGE_URL).headers(req_headers.clone()).send(),
                    )
                    .await?;
                }

            let status = response.status();
            let headers = response.headers().clone();
            let body = response
                .bytes()
                .await
                .map_err(|err| crate::upstream::network_failure(err, &scope))?;
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
    })
    .await
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
        http::header::USER_AGENT,
        HeaderValue::from_static(CLAUDE_CODE_UA),
    );
    headers.insert(
        super::HEADER_BETA,
        HeaderValue::from_static(OAUTH_BETA),
    );
    Ok(headers)
}
