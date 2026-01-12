use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use crate::context::AppContext;
use crate::formats::gemini::count_tokens::CountTokensResponse;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::model_get::ModelGetResponse;
use crate::formats::gemini::models_list::ModelsListResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::formats::gemini::types::Model as GeminiModel;
use crate::providers::common::usage::build_gemini_usage_record;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, GeminiCountTokens, GeminiGenerateContent, GeminiModelGet, GeminiModelsList,
    GeminiStreamGenerateContent, GeminiVersion, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_get_request_with_status, send_json_request_with_status,
};
use crate::providers::vertex::VertexProvider;
use crate::providers::vertex::auth::ensure_access_token;
use crate::providers::vertex::transform;
use super::{
    get_settings_and_credentials, vertex_location, vertex_model_id, vertex_publisher_model_path,
    vertex_version_path,
};

#[async_trait::async_trait]
impl GeminiGenerateContent for VertexProvider {
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
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let location = vertex_location(&provider.setting.base_url)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let model_path = vertex_publisher_model_path(&credential.project_id, &location, &model);
        let path = format!(
            "{}/{}:generateContent",
            vertex_version_path(version),
            model_path
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<GenerateContentResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        if let ParsedBody::Ok(ref response) = mapped.body
            && let Some(usage) = response.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::Vertex,
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
impl GeminiStreamGenerateContent for VertexProvider {
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
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let location = vertex_location(&provider.setting.base_url)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let model_path = vertex_publisher_model_path(&credential.project_id, &location, &model);
        let path = format!(
            "{}/{}:streamGenerateContent",
            vertex_version_path(version),
            model_path
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, true);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_sse_response::<StreamGenerateContentResponse>(res).await?;
        let usage_store = ctx.usage_store();
        let provider_credential_id = credential.project_id.clone();
        let model_name = model.clone();
        let mut recorded = false;
        let mapped = transform::map_sse_response(parsed, move |event| {
            if !recorded && let Some(usage) = event.usage_metadata.clone() {
                let record = build_gemini_usage_record(
                    ProviderKind::Vertex,
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
impl GeminiCountTokens for VertexProvider {
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
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let location = vertex_location(&provider.setting.base_url)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let model_path = vertex_publisher_model_path(&credential.project_id, &location, &model);
        let path = format!(
            "{}/{}:countTokens",
            vertex_version_path(version),
            model_path
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            &model,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
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
impl GeminiModelsList for VertexProvider {
    async fn gemini_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let path = format!("{}/publishers/google/models", vertex_version_path(version));
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<VertexPublisherModelsListResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, map_vertex_models_list)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl GeminiModelGet for VertexProvider {
    async fn gemini_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        version: GeminiVersion,
        name: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let model_id = vertex_model_id(&name);
        let path = format!(
            "{}/publishers/google/models/{model_id}",
            vertex_version_path(version)
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_gemini_query(&mut url, &query, false);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            name.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<VertexPublisherModel>(res).await?;
        let mapped = transform::map_json_response(parsed, map_vertex_model)?;
        render_json_response(mapped)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModel {
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, alias = "versionId")]
    version: Option<String>,
    #[serde(default)]
    input_token_limit: Option<i64>,
    #[serde(default)]
    output_token_limit: Option<i64>,
    #[serde(default)]
    supported_generation_methods: Option<Vec<String>>,
    #[serde(default)]
    thinking: Option<bool>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    max_temperature: Option<f64>,
    #[serde(default)]
    top_p: Option<f64>,
    #[serde(default)]
    top_k: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModelsListResponse {
    #[serde(default)]
    publisher_models: Vec<VertexPublisherModel>,
    #[serde(default)]
    next_page_token: Option<String>,
}

fn map_vertex_models_list(
    list: VertexPublisherModelsListResponse,
) -> Result<ModelsListResponse, StatusCode> {
    let models = list
        .publisher_models
        .into_iter()
        .map(map_vertex_model_inner)
        .collect();
    Ok(ModelsListResponse {
        models,
        next_page_token: list.next_page_token,
    })
}

fn map_vertex_model(model: VertexPublisherModel) -> Result<ModelGetResponse, StatusCode> {
    Ok(map_vertex_model_inner(model))
}

fn map_vertex_model_inner(model: VertexPublisherModel) -> GeminiModel {
    let id = vertex_model_id(&model.name);
    let version = model.version.unwrap_or_else(|| "unknown".to_string());
    GeminiModel {
        name: format!("models/{id}"),
        base_model_id: None,
        version,
        display_name: model.display_name,
        description: model.description,
        input_token_limit: model.input_token_limit,
        output_token_limit: model.output_token_limit,
        supported_generation_methods: model.supported_generation_methods,
        thinking: model.thinking,
        temperature: model.temperature,
        max_temperature: model.max_temperature,
        top_p: model.top_p,
        top_k: model.top_k,
    }
}
