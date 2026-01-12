use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Serialize;
use std::time::Duration;
use uuid::Uuid;

use crate::context::AppContext;
use crate::formats::gemini::count_tokens::CountTokensResponse;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::providers::common::gemini_models;
use crate::providers::common::usage::build_gemini_usage_record;
use crate::providers::credential_status::{DEFAULT_MODEL_KEY, ProviderKind};
use crate::providers::endpoints::{
    DownstreamRequest, GeminiCountTokens, GeminiGenerateContent, GeminiModelGet, GeminiModelsList,
    GeminiStreamGenerateContent, GeminiVersion,
};
use crate::providers::antigravity::constants::ANTIGRAVITY_USER_AGENT;
use crate::providers::antigravity::AntigravityProvider;
use crate::providers::antigravity::transform;
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, parse_json_response,
    parse_sse_response, render_json_response, render_sse_response,
    send_json_request_with_status, send_json_request_with_status_timeout,
};
use serde_json::Value;

#[derive(Serialize)]
pub(super) struct AntigravityGenerateRequest<'a, T> {
    pub(super) model: &'a str,
    pub(super) project: &'a str,
    pub(super) request: &'a T,
}

#[derive(Serialize)]
struct AntigravityCountTokensRequest<'a, T> {
    request: &'a T,
}

#[async_trait::async_trait]
impl GeminiGenerateContent for AntigravityProvider {
    async fn gemini_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::generate_content::GenerateContentRequest>,
        _version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let crate::providers::endpoints::UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(&model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let mut url = build_url(&provider.setting.base_url, "v1internal:generateContent")?;
        apply_gemini_query(&mut url, &query, false);
        let payload = AntigravityGenerateRequest {
            model: &model,
            project: credential.project_id.as_str(),
            request: &body,
        };

        let res = send_json_request_with_status_timeout(
            ctx,
            ProviderKind::Antigravity,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |headers| {
                apply_antigravity_headers(headers, &model)?;
                Ok(())
            },
            &payload,
            Duration::from_secs(300),
        )
        .await?;
        let parsed = parse_json_response::<Value>(res).await?;
        let mapped = transform::map_json_response(parsed, map_wrapped_json_response)?;
        if let ParsedBody::Ok(ref response) = mapped.body
            && let Some(usage) = response.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::Antigravity,
                    response.response_id.as_deref(),
                    &model,
                    caller_api_key.clone(),
                    credential.project_id.clone(),
                    usage,
                );
                let _ = ctx.usage_store().record(record).await;
            }

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiStreamGenerateContent for AntigravityProvider {
    async fn gemini_stream_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::generate_content::GenerateContentRequest>,
        _version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let crate::providers::endpoints::UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(&model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let mut url = build_url(&provider.setting.base_url, "v1internal:streamGenerateContent")?;
        apply_gemini_query(&mut url, &query, true);
        let payload = AntigravityGenerateRequest {
            model: &model,
            project: credential.project_id.as_str(),
            request: &body,
        };

        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Antigravity,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |headers| {
                apply_antigravity_headers(headers, &model)?;
                Ok(())
            },
            &payload,
        )
        .await?;
        let parsed = parse_sse_response::<Value>(res).await?;
        let usage_store = ctx.usage_store();
        let provider_credential_id = credential.project_id.clone();
        let model_name = model.clone();
        let mut recorded = false;
        let mapped = transform::map_sse_response(parsed, move |event| {
            let event = map_wrapped_sse_event(event)?;
            if !recorded && let Some(usage) = event.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::Antigravity,
                    event.response_id.as_deref(),
                    &model_name,
                    caller_api_key.clone(),
                    provider_credential_id.clone(),
                    usage,
                );
                recorded = true;
                let usage_store = usage_store.clone();
                tokio::spawn(async move {
                    let _ = usage_store.record(record).await;
                });
            }
            Ok(event)
        });

        render_sse_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiCountTokens for AntigravityProvider {
    async fn gemini_count_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::count_tokens::CountTokensRequest>,
        _version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let crate::providers::endpoints::UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(&model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let mut url = build_url(&provider.setting.base_url, "v1internal:countTokens")?;
        apply_gemini_query(&mut url, &query, false);
        let payload = AntigravityCountTokensRequest { request: &body };

        let res = send_json_request_with_status_timeout(
            ctx,
            ProviderKind::Antigravity,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |headers| {
                apply_antigravity_headers(headers, &model)?;
                Ok(())
            },
            &payload,
            Duration::from_secs(300),
        )
        .await?;
        let parsed = parse_json_response::<Value>(res).await?;
        let mapped = transform::map_json_response(parsed, |value| {
            let value = unwrap_response_value(value);
            serde_json::from_value::<CountTokensResponse>(value)
                .map_err(|_| StatusCode::BAD_GATEWAY)
        })?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiModelsList for AntigravityProvider {
    async fn gemini_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _version: GeminiVersion,
    ) -> Result<Response, StatusCode> {
        let crate::providers::endpoints::UpstreamRequest { headers, query, .. } =
            transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(DEFAULT_MODEL_KEY)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let mut url = build_url(&provider.setting.base_url, "v1internal:fetchAvailableModels")?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Antigravity,
            credential.project_id.as_str(),
            DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |headers| {
                apply_antigravity_headers(headers, "")?;
                Ok(())
            },
            &serde_json::json!({}),
        )
        .await?;
        let parsed = parse_json_response::<Value>(res).await?;
        let mapped = transform::map_json_response(parsed, gemini_models::map_models_list)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiModelGet for AntigravityProvider {
    async fn gemini_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _version: GeminiVersion,
        name: String,
    ) -> Result<Response, StatusCode> {
        let crate::providers::endpoints::UpstreamRequest { headers, query, .. } =
            transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential_for_model(DEFAULT_MODEL_KEY)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let mut url = build_url(&provider.setting.base_url, "v1internal:fetchAvailableModels")?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Antigravity,
            credential.project_id.as_str(),
            DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |headers| {
                apply_antigravity_headers(headers, "")?;
                Ok(())
            },
            &serde_json::json!({}),
        )
        .await?;
        let parsed = parse_json_response::<Value>(res).await?;
        let mapped = transform::map_json_response(parsed, |value| {
            gemini_models::map_model_get(value, &name)
        })?;
        render_json_response(mapped)
    }
}

pub(super) fn apply_antigravity_headers(
    headers: &mut axum::http::HeaderMap,
    model: &str,
) -> Result<(), StatusCode> {
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_static(ANTIGRAVITY_USER_AGENT),
    );
    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip"),
    );
    let request_id = format!("req-{}", Uuid::new_v4());
    let request_id = HeaderValue::from_str(&request_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert("requestId", request_id);
    if !model.trim().is_empty() {
        let request_type = if model.to_ascii_lowercase().contains("image") {
            "image_gen"
        } else {
            "agent"
        };
        headers.insert(
            "requestType",
            HeaderValue::from_static(request_type),
        );
    }
    Ok(())
}

fn unwrap_response_value(value: Value) -> Value {
    if let Value::Object(mut map) = value {
        if !map.contains_key("candidates")
            && let Some(inner) = map.remove("response") {
                return inner;
            }
        Value::Object(map)
    } else {
        value
    }
}

pub(super) fn map_wrapped_json_response(
    value: Value,
) -> Result<GenerateContentResponse, StatusCode> {
    let value = unwrap_response_value(value);
    serde_json::from_value(value).map_err(|_| StatusCode::BAD_GATEWAY)
}

pub(super) fn map_wrapped_sse_event(
    value: Value,
) -> Result<StreamGenerateContentResponse, StatusCode> {
    let value = unwrap_response_value(value);
    serde_json::from_value(value).map_err(|_| StatusCode::BAD_GATEWAY)
}

pub(super) fn normalize_generate_model_name(name: &str) -> String {
    let name = name.trim_start_matches('/');
    if let Some(stripped) = name.strip_prefix("publishers/google/models/") {
        return stripped.to_string();
    }
    if let Some(stripped) = name.strip_prefix("models/") {
        return stripped.to_string();
    }
    name.to_string()
}

// impl_google_access_token! expands to ensure_access_token(...) for google OAuth credentials.
crate::impl_google_access_token!(crate::providers::antigravity::AntigravityCredential, antigravity);
