use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Serialize;

use crate::context::AppContext;
use crate::formats::gemini::count_tokens::CountTokensResponse;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::model_get::ModelGetResponse;
use crate::formats::gemini::models_list::ModelsListResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::providers::common::usage::build_gemini_usage_record;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, GeminiCountTokens, GeminiGenerateContent, GeminiModelGet, GeminiModelsList,
    GeminiStreamGenerateContent, GeminiVersion, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_json_request_with_status,
};
use crate::providers::vertexexpress::VertexExpressProvider;
use crate::providers::vertexexpress::transform;

#[async_trait::async_trait]
impl GeminiGenerateContent for VertexExpressProvider {
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
        let model_path = vertex_express_model_path(&model);
        let path = format!(
            "{}/{}:generateContent",
            vertex_express_version_path(version),
            model_path
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::VertexExpress,
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
                    ProviderKind::VertexExpress,
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
impl GeminiStreamGenerateContent for VertexExpressProvider {
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
        let model_path = vertex_express_model_path(&model);
        let path = format!(
            "{}/{}:streamGenerateContent",
            vertex_express_version_path(version),
            model_path
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, true);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::VertexExpress,
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
                    ProviderKind::VertexExpress,
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
impl GeminiCountTokens for VertexExpressProvider {
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
        let model_path = vertex_express_model_path(&model);
        let path = format!(
            "{}/{}:countTokens",
            vertex_express_version_path(version),
            model_path
        );
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::VertexExpress,
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
impl GeminiModelsList for VertexExpressProvider {
    async fn gemini_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
    ) -> Result<Response, StatusCode> {
        let _ = (ctx, req, version);
        let list = load_models_list()?;
        json_response(&list)
    }
}

#[async_trait::async_trait]
impl GeminiModelGet for VertexExpressProvider {
    async fn gemini_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
        name: String,
    ) -> Result<Response, StatusCode> {
        let _ = (ctx, req, version);
        let target = normalize_model_name(&name);
        let list = load_models_list()?;
        let model = list
            .models
            .into_iter()
            .find(|item| normalize_model_name(&item.name) == target)
            .ok_or(StatusCode::NOT_FOUND)?;
        json_response::<ModelGetResponse>(&model)
    }
}

const MODELS_JSON: &str = include_str!("models.gemini.json");

pub(super) fn vertex_express_version_path(version: GeminiVersion) -> &'static str {
    match version {
        GeminiVersion::V1 => "/v1",
        GeminiVersion::V1Beta => "/v1beta1",
    }
}

pub(super) fn vertex_express_model_path(model: &str) -> String {
    let model = model.trim_start_matches('/');
    if model.starts_with("publishers/") {
        return model.to_string();
    }
    if let Some(stripped) = model.strip_prefix("models/") {
        return format!("publishers/google/models/{stripped}");
    }
    format!("publishers/google/models/{model}")
}

fn load_models_list() -> Result<ModelsListResponse, StatusCode> {
    serde_json::from_str(MODELS_JSON).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn normalize_model_name(name: &str) -> String {
    let name = name.trim_start_matches('/');
    if let Some(stripped) = name.strip_prefix("publishers/google/")
        && stripped.starts_with("models/") {
            return stripped.to_string();
        }
    if name.starts_with("models/") {
        return name.to_string();
    }
    format!("models/{name}")
}

fn json_response<T: Serialize>(value: &T) -> Result<Response, StatusCode> {
    let body = serde_json::to_vec(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = Response::new(Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}
