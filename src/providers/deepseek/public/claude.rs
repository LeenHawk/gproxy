use std::collections::HashMap;

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::context::AppContext;
use crate::formats::claude::count_tokens::CountTokensResponse;
use crate::formats::claude::messages::MessageCreateResponse;
use crate::formats::claude::models_list::ModelsListResponse;
use crate::formats::claude::stream::ClaudeStreamEvent;
use crate::formats::claude::types::{BetaModelInfo, ModelObjectType};
use crate::formats::openai::models_list::ListObjectType;
use crate::formats::openai::types::ModelObjectType as OpenAIModelObjectType;
use crate::providers::common::usage::{
    build_claude_stream_usage_record, build_claude_usage_record,
};
use crate::providers::deepseek::DeepSeekProvider;
use crate::providers::deepseek::tokenizer::count_tokens_for_request;
use crate::providers::deepseek::transform;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    ClaudeMessages, ClaudeMessagesCountTokens, ClaudeModelGet, ClaudeModelsList, DownstreamRequest,
    UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, apply_query, build_url, filter_response_headers, parse_json_response,
    parse_sse_response, render_json_response, render_sse_response, send_get_request_with_status,
    send_json_request_with_status,
};

#[async_trait::async_trait]
impl ClaudeMessages for DeepSeekProvider {
    async fn claude_messages(
        ctx: &AppContext,
        req: DownstreamRequest<crate::formats::claude::messages::MessageCreateRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/anthropic/v1/messages")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::DeepSeek,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<ClaudeStreamEvent>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let mut message_id: Option<String> = None;
            let mut message_model: Option<String> = None;
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                match &event {
                    ClaudeStreamEvent::MessageStart { message } => {
                        message_id = Some(message.id.clone());
                        message_model = Some(message.model.as_str().to_string());
                        if !recorded && let Some(usage) = message.usage.clone() {
                            let record = build_claude_stream_usage_record(
                                ProviderKind::DeepSeek,
                                message_id.as_deref(),
                                message_model.as_deref().unwrap_or_default(),
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
                    }
                    ClaudeStreamEvent::MessageDelta { usage, .. } => {
                        if !recorded && let Some(usage) = usage.clone() {
                            let record = build_claude_stream_usage_record(
                                ProviderKind::DeepSeek,
                                message_id.as_deref(),
                                message_model.as_deref().unwrap_or_default(),
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
                    }
                    _ => {}
                }
                Ok(event)
            });
            render_sse_response(mapped)
        } else {
            let parsed = parse_json_response::<MessageCreateResponse>(res).await?;
            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body {
                let record = build_claude_usage_record(
                    ProviderKind::DeepSeek,
                    Some(response.id.as_str()),
                    response.model.as_str(),
                    caller_api_key.clone(),
                    credential.key.clone(),
                    response.usage.clone(),
                );
                let _ = ctx.usage_store().record(record).await;
            }
            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl ClaudeMessagesCountTokens for DeepSeekProvider {
    async fn claude_messages_count_tokens(
        _ctx: &AppContext,
        req: DownstreamRequest<crate::formats::claude::count_tokens::CountTokensRequest>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { body, .. } = transform::to_upstream_request(req);
        let input_tokens =
            count_tokens_for_request(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let response = CountTokensResponse {
            context_management: None,
            input_tokens,
        };
        let body = serde_json::to_vec(&response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = StatusCode::OK;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(response)
    }
}

#[async_trait::async_trait]
impl ClaudeModelsList for DeepSeekProvider {
    async fn claude_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, .. } = transform::to_upstream_request(req);
        let models = match fetch_deepseek_models(ctx, &headers).await {
            Ok(models) => models,
            Err(response) => return Ok(response),
        };
        let data: Vec<BetaModelInfo> = models.iter().map(map_deepseek_model).collect();
        let first_id = data.first().map(|item| item.id.clone()).unwrap_or_default();
        let last_id = data.last().map(|item| item.id.clone()).unwrap_or_default();
        let body = ModelsListResponse {
            data,
            first_id,
            last_id,
            has_more: false,
        };
        let body = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = StatusCode::OK;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(response)
    }
}

#[async_trait::async_trait]
impl ClaudeModelGet for DeepSeekProvider {
    async fn claude_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, .. } = transform::to_upstream_request(req);
        let models = match fetch_deepseek_models(ctx, &headers).await {
            Ok(models) => models,
            Err(response) => return Ok(response),
        };
        let Some(model) = models.iter().find(|item| item.id == model_id) else {
            return Err(StatusCode::NOT_FOUND);
        };
        let body = map_deepseek_model(model);
        let body = serde_json::to_vec(&body).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = StatusCode::OK;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(response)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeepSeekModel {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: OpenAIModelObjectType,
    pub owned_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeepSeekModelsListResponse {
    #[serde(rename = "object")]
    pub object_type: ListObjectType,
    pub data: Vec<DeepSeekModel>,
}

fn map_deepseek_model(model: &DeepSeekModel) -> BetaModelInfo {
    let created_at = model
        .created
        .and_then(|value| OffsetDateTime::from_unix_timestamp(value).ok())
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    BetaModelInfo {
        id: model.id.clone(),
        created_at,
        display_name: model.id.clone(),
        model_type: ModelObjectType::Model,
    }
}

fn status_response(status: StatusCode) -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = status;
    response
}

async fn fetch_deepseek_models(
    ctx: &AppContext,
    headers: &HeaderMap,
) -> Result<Vec<DeepSeekModel>, Response> {
    let provider = super::get_settings_and_credentials(ctx)
        .await
        .map_err(status_response)?;
    let credential = provider
        .pick_credential()
        .ok_or_else(|| status_response(StatusCode::INTERNAL_SERVER_ERROR))?;
    let query = HashMap::new();
    let mut url = build_url(&provider.setting.base_url, "/models")
        .map_err(|_| status_response(StatusCode::INTERNAL_SERVER_ERROR))?;
    apply_query(&mut url, &query);

    let res = send_get_request_with_status(
        ctx,
        ProviderKind::DeepSeek,
        credential.key.as_str(),
        crate::providers::credential_status::DEFAULT_MODEL_KEY,
        ctx.http_client(),
        url.as_str(),
        headers,
        AuthMode::AuthorizationBearer,
        &credential.key,
        |_| Ok(()),
    )
    .await
    .map_err(status_response)?;

    let status = res.status();
    let resp_headers = filter_response_headers(res.headers());
    let body = res
        .bytes()
        .await
        .map_err(|_| status_response(StatusCode::BAD_GATEWAY))?;

    if !status.is_success() {
        let parsed: serde_json::Value =
            serde_json::from_slice(&body).map_err(|_| status_response(StatusCode::BAD_GATEWAY))?;
        let body =
            serde_json::to_vec(&parsed).map_err(|_| status_response(StatusCode::BAD_GATEWAY))?;
        let mut response = Response::new(Body::from(body));
        *response.status_mut() = status;
        *response.headers_mut() = resp_headers;
        return Err(response);
    }

    let parsed: DeepSeekModelsListResponse =
        serde_json::from_slice(&body).map_err(|_| status_response(StatusCode::BAD_GATEWAY))?;
    Ok(parsed.data)
}
