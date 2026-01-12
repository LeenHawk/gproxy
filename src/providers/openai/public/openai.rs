use axum::http::StatusCode;
use axum::response::Response;

use crate::context::AppContext;
use crate::formats::openai::chat_completions::{
    CompletionUsage, CreateChatCompletionRequest, CreateChatCompletionResponse,
    CreateChatCompletionStreamResponse,
};
use crate::formats::openai::conversations::{
    ConversationItem, ConversationItemList, ConversationResource, CreateConversationItemsRequest,
    CreateConversationRequest, DeletedConversationResource, UpdateConversationRequest,
};
use crate::formats::openai::model_get::ModelGetResponse;
use crate::formats::openai::models_list::ModelsListResponse;
use crate::formats::openai::responses::{
    CompactResponseRequest, CompactResponseResource, CreateResponseRequest, ResponseDeletedResource,
    ResponseItemList, ResponseObject, ResponseStreamEvent, ResponseUsage,
};
use crate::formats::openai::responses_input_tokens::{
    ResponseInputTokensRequest, ResponseInputTokensResponse,
};
use crate::providers::endpoints::{
    DownstreamRequest, OpenAIChatCompletions, OpenAIConversationItems, OpenAIConversations,
    OpenAIModelGet, OpenAIModelsList, OpenAIResponses, OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::openai::OpenAIProvider;
use crate::providers::openai::transform;
use crate::providers::credential_status::ProviderKind;
use crate::providers::router::{
    AuthMode, apply_query, build_url, parse_json_response, parse_sse_response,
    render_json_response, render_sse_response, send_delete_request_with_status,
    send_get_request_with_status, send_json_request_with_status, ParsedBody,
};
use crate::providers::common::usage::ensure_openai_stream_usage;
use crate::providers::usage::UsageRecord;

#[async_trait::async_trait]
impl OpenAIChatCompletions for OpenAIProvider {
    async fn openai_chat_completions(
        ctx: &AppContext,
        req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let mut body = body;
        ensure_openai_stream_usage(&mut body);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<CreateChatCompletionStreamResponse>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let caller_api_key = caller_api_key.clone();
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                if !recorded && let Some(usage) = event.usage.clone() {
                    let record = build_chat_usage_record(
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
                    let record = build_chat_usage_record(
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
impl OpenAIResponses for OpenAIProvider {
    async fn openai_responses(
        ctx: &AppContext,
        req: DownstreamRequest<CreateResponseRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            body
                .model
                .as_deref()
                .unwrap_or(crate::providers::credential_status::DEFAULT_MODEL_KEY),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        if body.stream == Some(true) {
            let parsed = parse_sse_response::<ResponseStreamEvent>(res).await?;
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let caller_api_key = caller_api_key.clone();
            let mapped = transform::map_sse_response(parsed, move |event| {
                if let ResponseStreamEvent::ResponseCompleted { response, .. } = &event
                    && let Some(usage) = response.usage.clone() {
                        let record = build_response_usage_record(
                            response,
                            caller_api_key.clone(),
                            provider_credential_id.clone(),
                            usage,
                        );
                        let usage_store = usage_store.clone();
                        tokio::spawn(async move {
                            let _ = usage_store.record(record).await;
                        });
                    }
                Ok(event)
            });
            render_sse_response(mapped)
        } else {
            let parsed = parse_json_response::<ResponseObject>(res).await?;
            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage.clone() {
                    let record = build_response_usage_record(
                        response,
                        caller_api_key.clone(),
                        credential.key.clone(),
                        usage,
                    );
                    let _ = ctx.usage_store().record(record).await;
                }
            render_json_response(mapped)
        }
    }

    async fn openai_responses_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let stream = query
            .get("stream")
            .map(|value| value == "true" || value == "1")
            .unwrap_or(false);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        if stream {
            let parsed = parse_sse_response::<ResponseStreamEvent>(res).await?;
            let mapped = transform::map_sse_response(parsed, Ok);
            render_sse_response(mapped)
        } else {
            let parsed = parse_json_response::<ResponseObject>(res).await?;
            let mapped = transform::map_json_response(parsed, Ok)?;
            render_json_response(mapped)
        }
    }

    async fn openai_responses_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ResponseDeletedResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_responses_cancel(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let empty_body = serde_json::Map::<String, serde_json::Value>::new();
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &empty_body,
        )
        .await?;
        let parsed = parse_json_response::<ResponseObject>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_responses_compact(
        ctx: &AppContext,
        req: DownstreamRequest<CompactResponseRequest>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<CompactResponseResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_responses_input_items_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _response_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ResponseItemList>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIResponsesInputTokens for OpenAIProvider {
    async fn openai_responses_input_tokens(
        ctx: &AppContext,
        req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            body
                .model
                .as_deref()
                .unwrap_or(crate::providers::credential_status::DEFAULT_MODEL_KEY),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<ResponseInputTokensResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIConversations for OpenAIProvider {
    async fn openai_conversations_create(
        ctx: &AppContext,
        req: DownstreamRequest<CreateConversationRequest>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<ConversationResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversations_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ConversationResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversations_update(
        ctx: &AppContext,
        req: DownstreamRequest<UpdateConversationRequest>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<ConversationResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversations_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<DeletedConversationResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIConversationItems for OpenAIProvider {
    async fn openai_conversation_items_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ConversationItemList>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversation_items_create(
        ctx: &AppContext,
        req: DownstreamRequest<CreateConversationItemsRequest>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
            &body,
        )
        .await?;
        let parsed = parse_json_response::<ConversationItemList>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversation_items_retrieve(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _conversation_id: String,
        _item_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ConversationItem>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }

    async fn openai_conversation_items_delete(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        _conversation_id: String,
        _item_id: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ConversationResource>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for OpenAIProvider {
    async fn openai_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
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
impl OpenAIModelGet for OpenAIProvider {
    async fn openai_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest {
            headers,
            query,
            path,
            ..
        } = transform::to_upstream_request(req);
        let _ = model;
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::OpenAI,
            credential.key.as_str(),
            model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.key.as_str(),
            |_| Ok(()),
        )
        .await?;
        let parsed = parse_json_response::<ModelGetResponse>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

fn build_chat_usage_record(
    request_id: &str,
    created_at: i64,
    model: &str,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: CompletionUsage,
) -> UsageRecord {
    UsageRecord {
        provider: ProviderKind::OpenAI,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(model.to_string()),
        upstream_model: None,
        format: Some("openai".to_string()),
        request_id: Some(request_id.to_string()),
        created_at,
        oa_resp_input_tokens: None,
        oa_resp_output_tokens: None,
        oa_resp_total_tokens: None,
        oa_resp_input_tokens_details_cached_tokens: None,
        oa_resp_output_tokens_details_reasoning_tokens: None,
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: Some(usage.prompt_tokens),
        oa_chat_completion_tokens: Some(usage.completion_tokens),
        oa_chat_total_tokens: Some(usage.total_tokens),
        oa_chat_prompt_tokens_details_cached_tokens: usage
            .prompt_tokens_details
            .and_then(|details| details.cached_tokens),
        oa_chat_completion_tokens_details_reasoning_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.reasoning_tokens),
        oa_chat_completion_tokens_details_audio_tokens: usage
            .completion_tokens_details
            .as_ref()
            .and_then(|details| details.audio_tokens),
        claude_input_tokens: None,
        claude_output_tokens: None,
        claude_cache_creation_input_tokens: None,
        claude_cache_read_input_tokens: None,
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}

fn build_response_usage_record(
    response: &ResponseObject,
    caller_api_key: Option<String>,
    provider_credential_id: String,
    usage: ResponseUsage,
) -> UsageRecord {
    UsageRecord {
        provider: ProviderKind::OpenAI,
        caller_api_key,
        provider_credential_id: Some(provider_credential_id),
        model: Some(response.model.clone()),
        upstream_model: None,
        format: Some("openai".to_string()),
        request_id: Some(response.id.clone()),
        created_at: response.created_at,
        oa_resp_input_tokens: Some(usage.input_tokens),
        oa_resp_output_tokens: Some(usage.output_tokens),
        oa_resp_total_tokens: Some(usage.total_tokens),
        oa_resp_input_tokens_details_cached_tokens: Some(usage.input_tokens_details.cached_tokens),
        oa_resp_output_tokens_details_reasoning_tokens: Some(
            usage.output_tokens_details.reasoning_tokens,
        ),
        oa_resp_output_tokens_details_audio_tokens: None,
        oa_resp_output_tokens_details_cached_tokens: None,
        oa_chat_prompt_tokens: None,
        oa_chat_completion_tokens: None,
        oa_chat_total_tokens: None,
        oa_chat_prompt_tokens_details_cached_tokens: None,
        oa_chat_completion_tokens_details_reasoning_tokens: None,
        oa_chat_completion_tokens_details_audio_tokens: None,
        claude_input_tokens: None,
        claude_output_tokens: None,
        claude_cache_creation_input_tokens: None,
        claude_cache_read_input_tokens: None,
        gemini_prompt_token_count: None,
        gemini_candidates_token_count: None,
        gemini_total_token_count: None,
        gemini_cached_content_token_count: None,
    }
}
