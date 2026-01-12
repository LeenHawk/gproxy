use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::Response;

use crate::context::AppContext;
use crate::formats::claude::model_get::ModelGetResponse as ClaudeModelGetResponse;
use crate::formats::claude::models_list::ModelsListResponse as ClaudeModelsListResponse;
use crate::formats::claude::types::BetaModelInfo;
use crate::formats::openai::chat_completions::{
    CreateChatCompletionRequest, CreateChatCompletionResponse, CreateChatCompletionStreamResponse,
};
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::model_get::ModelGetResponse;
use crate::formats::openai::models_list::{ListObjectType, ModelsListResponse};
use crate::formats::openai::responses::{CompactResponseRequest, CreateResponseRequest};
use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;
use crate::formats::openai::types::{Model, ModelObjectType};
use crate::providers::claude::ClaudeProvider;
use crate::providers::claude::transform;
use crate::providers::common::usage::{
    build_openai_chat_usage_record, ensure_openai_stream_usage,
};
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, OpenAIChatCompletions, OpenAIConversationItems, OpenAIConversations,
    OpenAIModelGet, OpenAIModelsList, OpenAIResponses, OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, apply_query, build_url, not_implemented_response, parse_json_response,
    parse_sse_response, render_json_response, render_sse_response, send_get_request_with_status,
    send_json_request_with_status,
};

#[async_trait::async_trait]
impl OpenAIChatCompletions for ClaudeProvider {
    async fn openai_chat_completions(
        ctx: &AppContext,
        req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            mut headers,
            query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let mut body = body;
        ensure_openai_stream_usage(&mut body);
        if headers.get("anthropic-version").is_none() {
            headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        }
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/chat/completions")?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<CreateChatCompletionStreamResponse>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                if !recorded && let Some(usage) = event.usage.clone() {
                    let record = build_openai_chat_usage_record(
                        ProviderKind::Claude,
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
            let parsed = parse_json_response::<CreateChatCompletionResponse>(res).await?;

            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage.clone() {
                    let record = build_openai_chat_usage_record(
                        ProviderKind::Claude,
                        &response.id,
                        response.created,
                        &response.model,
                        caller_api_key.clone(),
                        credential.key.clone(),
                        usage,
                    );
                    let _ = ctx.usage_store().record(record).await;
                }

            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl OpenAIResponses for ClaudeProvider {
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
impl OpenAIResponsesInputTokens for ClaudeProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for ClaudeProvider {
    async fn openai_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1/models")?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<ClaudeModelsListResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, map_claude_models_list)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIConversations for ClaudeProvider {
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

#[async_trait::async_trait]
impl OpenAIConversationItems for ClaudeProvider {
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

#[async_trait::async_trait]
impl OpenAIModelGet for ClaudeProvider {
    async fn openai_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let model = normalize_model_id(&model);
        let path = format!("/v1/models/{model}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let version = super::anthropic_version(&headers)?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Claude,
            credential.key.as_str(),
            model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::XApiKey,
            credential.key.as_str(),
            |out_headers| {
                out_headers.insert("anthropic-version", version);
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<ClaudeModelGetResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, map_claude_model)?;
        render_json_response(mapped)
    }
}

fn map_claude_models_list(
    list: ClaudeModelsListResponse,
) -> Result<ModelsListResponse, StatusCode> {
    let data = list.data.into_iter().map(map_claude_model_inner).collect();
    Ok(ModelsListResponse {
        object_type: ListObjectType::List,
        data,
    })
}

fn map_claude_model(model: ClaudeModelGetResponse) -> Result<ModelGetResponse, StatusCode> {
    Ok(map_claude_model_inner(model))
}

fn map_claude_model_inner(model: BetaModelInfo) -> Model {
    Model {
        id: model.id,
        created: model.created_at.unix_timestamp(),
        object_type: ModelObjectType::Model,
        owned_by: "anthropic".to_string(),
    }
}

fn normalize_model_id(model: &str) -> String {
    let model = model.trim_start_matches('/');
    model.strip_prefix("models/").unwrap_or(model).to_string()
}
