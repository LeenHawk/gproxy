use http::StatusCode;
use serde_json::json;

use gproxy_provider_core::{
    AttemptFailure, CredentialEntry, CredentialPool, DisallowScope, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::UpstreamOk;
use crate::record::headers_to_json;
use crate::upstream::{classify_status, send_with_logging};

use super::{
    build_headers, build_url, channel_base_url, credential_refresh_token, invalid_credential,
    PROVIDER_NAME,
};
use super::refresh;

pub(super) async fn handle_usage(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<UpstreamOk, UpstreamPassthroughError> {
    handle_usage_with(pool, ctx, None).await
}

pub(super) async fn handle_usage_for_credential(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
    credential_id: i64,
) -> Result<UpstreamOk, UpstreamPassthroughError> {
    let id = credential_id.to_string();
    handle_usage_with(pool, ctx, Some(id.as_str())).await
}

async fn handle_usage_with(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
    credential_id: Option<&str>,
) -> Result<UpstreamOk, UpstreamPassthroughError> {
    let scope = DisallowScope::AllModels;
    let runner = |credential: CredentialEntry<BaseCredential>| {
        let ctx = ctx.clone();
        let scope = scope.clone();
        async move {
            let tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
            let mut access_token = tokens.access_token.clone();
            let refresh_url = refresh::refresh_token_url(&ctx)
                .await
                .unwrap_or_else(|_| "https://oauth2.googleapis.com/token".to_string());
            let refresh_token = tokens
                .refresh_token
                .clone()
                .or_else(|| credential_refresh_token(credential.value()));
            let base_url = channel_base_url(&ctx).await.map_err(|err| AttemptFailure {
                passthrough: err,
                mark: None,
            })?;
            let path = "/v1internal:fetchAvailableModels".to_string();
            let url = build_url(Some(&base_url), &path);
            let client = shared_client(ctx.proxy.as_deref())?;
            let request_body = json!({});
            let request_body_str = serde_json::to_string(&request_body).unwrap_or_else(|_| "{}".to_string());

            let mut req_headers = build_headers(&access_token, "")?;
            let request_headers = headers_to_json(&req_headers);
            let mut response = send_with_logging(
                &ctx,
                PROVIDER_NAME,
                "antigravity.usage",
                "POST",
                &path,
                None,
                false,
                &scope,
                || {
                    client
                        .post(url.clone())
                        .headers(req_headers.clone())
                        .json(&request_body)
                        .send()
                },
            )
            .await?;

            if response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::FORBIDDEN
            {
                if let Some(refresh_token) = refresh_token {
                    let refreshed = refresh::refresh_access_token(
                        credential.value().id,
                        refresh_token,
                        &refresh_url,
                        &ctx,
                        &scope,
                    )
                    .await?;
                    access_token = refreshed.access_token;
                    req_headers = build_headers(&access_token, "")?;
                    response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "antigravity.usage",
                        "POST",
                        &path,
                        None,
                        false,
                        &scope,
                        || {
                            client
                                .post(url.clone())
                                .headers(req_headers.clone())
                                .json(&request_body)
                                .send()
                        },
                    )
                    .await?;
                } else {
                    return Err(invalid_credential(&scope, "missing refresh_token"));
                }
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

            let meta = UpstreamRecordMeta {
                provider: PROVIDER_NAME.to_string(),
                provider_id: ctx.provider_id,
                credential_id: Some(credential.value().id),
                operation: "antigravity.usage".to_string(),
                model: None,
                request_method: "POST".to_string(),
                request_path: path,
                request_query: None,
                request_headers,
                request_body: request_body_str,
            };
            Ok(UpstreamOk {
                response: ProxyResponse::Json {
                    status,
                    headers,
                    body,
                },
                meta,
            })
        }
    };

    match credential_id {
        Some(id) => pool.execute_for_id(id, scope.clone(), runner).await,
        None => pool.execute(scope.clone(), runner).await,
    }
}
