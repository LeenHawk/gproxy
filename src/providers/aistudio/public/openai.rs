use axum::http::StatusCode;
use axum::response::Response;

use crate::context::AppContext;
use crate::formats::openai::chat_completions::{
    CreateChatCompletionRequest, CreateChatCompletionResponse, CreateChatCompletionStreamResponse,
};
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::model_get::ModelGetResponse;
use crate::formats::openai::models_list::ModelsListResponse;
use crate::formats::openai::responses::{CompactResponseRequest, CreateResponseRequest};
use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;
use crate::providers::aistudio::AIStudioProvider;
use crate::providers::aistudio::transform;
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
impl OpenAIChatCompletions for AIStudioProvider {
    async fn openai_chat_completions(
        ctx: &AppContext,
        req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query: _query,
            body,
            ..
        } = transform::to_upstream_request(req);
        let mut body = body;
        ensure_openai_stream_usage(&mut body);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let url = build_url(&provider.setting.base_url, "/v1beta/openai/chat/completions")?;
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::AIStudio,
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
            let mut recorded = false;
            let mapped = transform::map_sse_response(parsed, move |event| {
                if !recorded && let Some(usage) = event.usage.clone() {
                    let record = build_openai_chat_usage_record(
                        ProviderKind::AIStudio,
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
                        ProviderKind::AIStudio,
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
impl OpenAIResponses for AIStudioProvider {
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
impl OpenAIResponsesInputTokens for AIStudioProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for AIStudioProvider {
    async fn openai_models_list(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, "/v1beta/openai/models")?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::AIStudio,
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
impl OpenAIModelGet for AIStudioProvider {
    async fn openai_model_get(
        ctx: &AppContext,
        req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode> {
        let UpstreamRequest { headers, query, .. } = transform::to_upstream_request(req);
        let model = normalize_model_id(&model);
        let path = format!("/v1beta/openai/models/{model}");
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::AIStudio,
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

fn normalize_model_id(model: &str) -> String {
    let model = model.trim_start_matches('/');
    model.strip_prefix("models/").unwrap_or(model).to_string()
}

#[async_trait::async_trait]
impl OpenAIConversations for AIStudioProvider {
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
impl OpenAIConversationItems for AIStudioProvider {
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
