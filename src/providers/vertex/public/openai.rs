use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::context::AppContext;
use crate::formats::openai::chat_completions::{
    ChatCompletionChoice, ChatCompletionFinishReason, ChatCompletionLogprobs,
    ChatCompletionObjectType, ChatCompletionResponseMessage, ChatCompletionResponseRole,
    CompletionTokensDetails, CompletionUsage, CreateChatCompletionRequest,
    CreateChatCompletionResponse, CreateChatCompletionStreamResponse, PromptTokensDetails,
};
use crate::formats::openai::types::ServiceTier;
use serde_json::Value;
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::model_get::ModelGetResponse;
use crate::formats::openai::models_list::{ListObjectType, ModelsListResponse};
use crate::formats::openai::responses::{CompactResponseRequest, CreateResponseRequest};
use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;
use crate::formats::openai::types::{Model, ModelObjectType};
use crate::providers::common::usage::{
    build_openai_chat_usage_record, ensure_openai_stream_usage,
};
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, GeminiVersion, OpenAIChatCompletions, OpenAIConversationItems,
    OpenAIConversations, OpenAIModelGet, OpenAIModelsList, OpenAIResponses,
    OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, apply_query, build_url, parse_json_response, parse_sse_response,
    render_json_response, render_sse_response, send_get_request_with_status,
    send_json_request_with_status, not_implemented_response,
};
use crate::providers::vertex::VertexProvider;
use crate::providers::vertex::auth::ensure_access_token;
use crate::providers::vertex::transform;
use super::{
    get_settings_and_credentials, vertex_location, vertex_model_id, vertex_openai_endpoint_path,
    vertex_version_path,
};

#[derive(Debug, Deserialize)]
struct VertexChatCompletionResponse {
    pub id: String,
    pub choices: Vec<VertexChatCompletionChoice>,
    pub created: i64,
    pub model: String,
    pub service_tier: Option<ServiceTier>,
    pub system_fingerprint: Option<String>,
    #[serde(rename = "object")]
    pub object_type: ChatCompletionObjectType,
    pub usage: Option<VertexCompletionUsage>,
}

#[derive(Debug, Deserialize)]
struct VertexChatCompletionChoice {
    pub finish_reason: ChatCompletionFinishReason,
    pub index: i64,
    #[serde(default)]
    pub message: Option<ChatCompletionResponseMessage>,
    pub logprobs: Option<ChatCompletionLogprobs>,
}

#[derive(Debug, Deserialize)]
struct VertexCompletionUsage {
    pub completion_tokens: Option<i64>,
    pub prompt_tokens: i64,
    pub total_tokens: i64,
    pub completion_tokens_details: Option<CompletionTokensDetails>,
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    pub extra_properties: Option<Value>,
}

fn empty_assistant_message() -> ChatCompletionResponseMessage {
    ChatCompletionResponseMessage {
        content: None,
        refusal: None,
        tool_calls: None,
        annotations: None,
        role: ChatCompletionResponseRole::Assistant,
        function_call: None,
        audio: None,
    }
}

#[async_trait::async_trait]
impl OpenAIChatCompletions for VertexProvider {
    async fn openai_chat_completions(
        ctx: &AppContext,
        req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        if body.model.trim().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        let mut body = body;
        body.model = normalize_vertex_openai_model(&body.model);
        ensure_openai_stream_usage(&mut body);
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let location = vertex_location(&provider.setting.base_url)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let endpoint_path = vertex_openai_endpoint_path(&credential.project_id, &location);
        let path = format!(
            "{}/{}/chat/completions",
            vertex_version_path(GeminiVersion::V1Beta),
            endpoint_path
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            access_token.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<CreateChatCompletionStreamResponse>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.project_id.clone();
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                if !recorded && let Some(usage) = event.usage.clone() {
                    let record = build_openai_chat_usage_record(
                        ProviderKind::Vertex,
                        &event.id,
                        event.created,
                        &event.model,
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
        } else {
            let parsed = parse_json_response::<VertexChatCompletionResponse>(res).await?;
            let mapped = transform::map_json_response(parsed, |response| {
                let choices = response
                    .choices
                    .into_iter()
                    .map(|choice| ChatCompletionChoice {
                        finish_reason: choice.finish_reason,
                        index: choice.index,
                        message: choice.message.unwrap_or_else(empty_assistant_message),
                        logprobs: choice.logprobs,
                    })
                    .collect();
                let usage = response.usage.map(|usage| CompletionUsage {
                    completion_tokens: usage
                        .completion_tokens
                        .unwrap_or_else(|| usage.total_tokens.saturating_sub(usage.prompt_tokens)),
                    prompt_tokens: usage.prompt_tokens,
                    total_tokens: usage.total_tokens,
                    completion_tokens_details: usage.completion_tokens_details,
                    prompt_tokens_details: usage.prompt_tokens_details,
                    extra_properties: usage.extra_properties,
                });
                Ok(CreateChatCompletionResponse {
                    id: response.id,
                    choices,
                    created: response.created,
                    model: response.model,
                    service_tier: response.service_tier,
                    system_fingerprint: response.system_fingerprint,
                    object_type: response.object_type,
                    usage,
                })
            })?;
            if let crate::providers::router::ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage.clone() {
                    let record = build_openai_chat_usage_record(
                        ProviderKind::Vertex,
                        &response.id,
                        response.created,
                        &response.model,
                        caller_api_key.clone(),
                        credential.project_id.clone(),
                        usage,
                    );
                    let _ = ctx.usage_store().record(record).await;
                }
            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl OpenAIResponses for VertexProvider {
    async fn openai_responses(
        _ctx: &AppContext,
        _req: DownstreamRequest<CreateResponseRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_responses_retrieve(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_responses_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_responses_cancel(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_responses_compact(
        _ctx: &AppContext,
        _req: DownstreamRequest<CompactResponseRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_responses_input_items_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIResponsesInputTokens for VertexProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for VertexProvider {
    async fn openai_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let path = format!(
            "{}/publishers/google/models",
            vertex_version_path(GeminiVersion::V1Beta)
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
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
impl OpenAIModelGet for VertexProvider {
    async fn openai_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let access_token = ensure_access_token(ctx, credential).await?;
        let model_id = vertex_model_id(&model);
        let path = format!(
            "{}/publishers/google/models/{model_id}",
            vertex_version_path(GeminiVersion::V1Beta)
        );
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Vertex,
            credential.project_id.as_str(),
            model.as_str(),
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

#[async_trait::async_trait]
impl OpenAIConversations for VertexProvider {
    async fn openai_conversations_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<CreateConversationRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversations_retrieve(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversations_update(
        _ctx: &AppContext,
        _req: DownstreamRequest<UpdateConversationRequest>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversations_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

fn normalize_vertex_openai_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_prefix("publishers/")
        && let Some((publisher, model_name)) = stripped.split_once("/models/") {
            return format!("{publisher}/{model_name}");
        }
    if let Some(idx) = trimmed.find("/publishers/") {
        let tail = &trimmed[(idx + "/publishers/".len())..];
        if let Some((publisher, model_name)) = tail.split_once("/models/") {
            return format!("{publisher}/{model_name}");
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("models/") {
        return format!("google/{stripped}");
    }
    if trimmed.contains('/') {
        return trimmed.to_string();
    }
    format!("google/{trimmed}")
}

#[async_trait::async_trait]
impl OpenAIConversationItems for VertexProvider {
    async fn openai_conversation_items_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversation_items_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<CreateConversationItemsRequest>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversation_items_retrieve(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _conversation_id: String,
        _item_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversation_items_delete(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _conversation_id: String,
        _item_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModel {
    name: String,
    #[serde(default)]
    create_time: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModelsListResponse {
    #[serde(default)]
    publisher_models: Vec<VertexPublisherModel>,
}

fn map_vertex_models_list(
    list: VertexPublisherModelsListResponse,
) -> Result<ModelsListResponse, StatusCode> {
    let data = list
        .publisher_models
        .into_iter()
        .map(map_vertex_model_inner)
        .collect();
    Ok(ModelsListResponse {
        object_type: ListObjectType::List,
        data,
    })
}

fn map_vertex_model(model: VertexPublisherModel) -> Result<ModelGetResponse, StatusCode> {
    Ok(map_vertex_model_inner(model))
}

fn map_vertex_model_inner(model: VertexPublisherModel) -> Model {
    let id = vertex_model_id(&model.name);
    let created = model
        .create_time
        .as_deref()
        .and_then(parse_rfc3339)
        .unwrap_or(0);
    Model {
        id: id.to_string(),
        created,
        object_type: ModelObjectType::Model,
        owned_by: "google".to_string(),
    }
}

fn parse_rfc3339(value: &str) -> Option<i64> {
    OffsetDateTime::parse(value, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}
