use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{json, Value as JsonValue};

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, ProxyResponse, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::UpstreamOk;
use crate::upstream::{classify_status, send_with_logging};

use super::{
    build_usage_url, build_codex_json_headers, credential_account_id, credential_base_url,
    credential_refresh_token, invalid_credential, PROVIDER_NAME,
};
use super::refresh;

struct UsageFetch {
    payload: JsonValue,
    credential_id: i64,
}

pub(super) async fn fetch_usage_payload(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<JsonValue, UpstreamPassthroughError> {
    let result = fetch_usage_payload_with_credential(pool, ctx).await?;
    Ok(result.payload)
}

pub(super) async fn handle_usage(
    pool: &CredentialPool<BaseCredential>,
    ctx: UpstreamContext,
) -> Result<UpstreamOk, UpstreamPassthroughError> {
    let result = fetch_usage_payload_with_credential(pool, ctx.clone()).await?;
    let summary = summarize_usage(result.payload);
    let body_bytes = serde_json::to_vec(&summary)
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
        operation: "codex.usage".to_string(),
        model: None,
        request_method: "GET".to_string(),
        request_path: "/codex/usage".to_string(),
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
            let tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
            let mut access_token = tokens.access_token.clone();
            let refresh_token = tokens
                .refresh_token
                .clone()
                .or_else(|| credential_refresh_token(credential.value()));
            let account_id = credential_account_id(credential.value())
                .ok_or_else(|| invalid_credential(&scope, "missing account_id"))?;
            let base_url = credential_base_url(credential.value());
            let (url, path) = build_usage_url(base_url.as_deref());
            let url_req = url.clone();
            let client = shared_client(ctx.proxy.as_deref())?;
            let mut req_headers = build_codex_json_headers(&access_token, &account_id)?;
            let mut response = send_with_logging(
                &ctx,
                PROVIDER_NAME,
                "codex.usage",
                "GET",
                &path,
                None,
                false,
                &scope,
                || client.get(url_req).headers(req_headers.clone()).send(),
            )
            .await?;
            if response.status() == StatusCode::UNAUTHORIZED
                || response.status() == StatusCode::FORBIDDEN
            {
                if let Some(refresh_token) = refresh_token {
                    let refreshed =
                        refresh::refresh_access_token(credential.value().id, refresh_token, &ctx, &scope)
                            .await?;
                    access_token = refreshed.access_token;
                    req_headers = build_codex_json_headers(&access_token, &account_id)?;
                    response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "codex.usage",
                        "GET",
                        &path,
                        None,
                        false,
                        &scope,
                        || client.get(url.clone()).headers(req_headers.clone()).send(),
                    )
                    .await?;
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

fn summarize_usage(payload: JsonValue) -> JsonValue {
    let plan_type = payload.get("plan_type").cloned().unwrap_or(JsonValue::Null);
    let rate_limit = payload.get("rate_limit").cloned().unwrap_or(JsonValue::Null);
    let primary = rate_limit
        .get("primary_window")
        .cloned()
        .unwrap_or(JsonValue::Null);
    let secondary = rate_limit
        .get("secondary_window")
        .cloned()
        .unwrap_or(JsonValue::Null);
    json!({
        "plan_type": plan_type,
        "primary_window": primary,
        "secondary_window": secondary,
        "raw": payload
    })
}
