use axum::http::StatusCode;
use axum::response::Response;

use crate::context::AppContext;
use crate::formats::gemini::count_tokens::CountTokensResponse;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::model_get::ModelGetResponse;
use crate::formats::gemini::models_list::ModelsListResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::providers::aistudio::AIStudioProvider;
use crate::providers::aistudio::transform;
use crate::providers::common::usage::build_gemini_usage_record;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, GeminiCountTokens, GeminiGenerateContent, GeminiModelGet, GeminiModelsList,
    GeminiStreamGenerateContent, GeminiVersion, UpstreamRequest, gemini_version_path,
};
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_get_request_with_status, send_json_request_with_status,
};

#[async_trait::async_trait]
impl GeminiGenerateContent for AIStudioProvider {
    async fn gemini_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::generate_content::GenerateContentRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let path = format!(
            "{}/models/{model}:generateContent",
            gemini_version_path(version, "")
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::AIStudio,
            credential.key.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XGoogApiKey,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<GenerateContentResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;
        if let ParsedBody::Ok(ref response) = mapped.body
            && let Some(usage) = response.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::AIStudio,
                    response.response_id.as_deref(),
                    &model,
                    caller_api_key.clone(),
                    credential.key.clone(),
                    usage,
                );
                let _ = ctx.usage_store().record(record).await;
            }

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiStreamGenerateContent for AIStudioProvider {
    async fn gemini_stream_generate_content(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::generate_content::GenerateContentRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let path = format!(
            "{}/models/{model}:streamGenerateContent",
            gemini_version_path(version, "")
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, true);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::AIStudio,
            credential.key.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XGoogApiKey,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_sse_response::<StreamGenerateContentResponse>(res).await?;
        let usage_store = ctx.usage_store();
        let provider_credential_id = credential.key.clone();
        let model_name = model.clone();
        let mut recorded = false;
        let mapped = transform::map_sse_response(parsed, move |event| {
            if !recorded && let Some(usage) = event.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::AIStudio,
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
impl GeminiCountTokens for AIStudioProvider {
    async fn gemini_count_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::gemini::count_tokens::CountTokensRequest>,
        version: GeminiVersion,
        model: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let path = format!(
            "{}/models/{model}:countTokens",
            gemini_version_path(version, "")
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::AIStudio,
            credential.key.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XGoogApiKey,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<CountTokensResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiModelsList for AIStudioProvider {
    async fn gemini_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let path = gemini_version_path(version, "/models");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::AIStudio,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XGoogApiKey,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ModelsListResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiModelGet for AIStudioProvider {
    async fn gemini_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
        name: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let path = format!("{}/models/{name}", gemini_version_path(version, ""));
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::AIStudio,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XGoogApiKey,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ModelGetResponse>(res).await?;

        let mapped = transform::map_json_response(parsed, Ok)?;

        render_json_response(mapped)
    }
}
