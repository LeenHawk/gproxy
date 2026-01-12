use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use serde_json::Value;
use uuid::Uuid;

use crate::context::AppContext;
use crate::formats::gemini::generate_content::GenerateContentResponse;
use crate::formats::gemini::query::apply_gemini_query;
use crate::formats::gemini::stream_generate_content::StreamGenerateContentResponse;
use crate::formats::openai::chat_completions::CreateChatCompletionRequest;
use crate::providers::common::usage::{
    build_gemini_usage_record, ensure_openai_stream_usage,
};
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, GeminiVersion, OpenAIChatCompletions, OpenAIConversationItems,
    OpenAIConversations, OpenAIModelGet, OpenAIModelsList, OpenAIResponses,
    OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, build_url, not_implemented_response, parse_json_response,
    parse_sse_response, render_json_response, render_sse_response, send_json_request_with_status,
};
use crate::providers::vertexexpress::VertexExpressProvider;
use crate::providers::vertexexpress::transform;

#[async_trait::async_trait]
impl OpenAIChatCompletions for VertexExpressProvider {
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

        let mut body = body;
        ensure_openai_stream_usage(&mut body);

        let request_model = body.model.clone();
        let stream = body.stream == Some(true);

        let gemini_request =
            crate::formats::transform::gen_openai_chat_to_gemini_generate::request(body)
                .map_err(|_| StatusCode::BAD_REQUEST)?;

        let model_path = super::gemini::vertex_express_model_path(&request_model);
        let version = GeminiVersion::V1Beta;

        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        if stream {
            let path = format!(
                "{}/{}:streamGenerateContent",
                super::gemini::vertex_express_version_path(version),
                model_path
            );
            let mut url = build_url(&provider.setting.base_url, &path)?;
            apply_gemini_query(&mut url, &query, true);
            let res = send_json_request_with_status(
                ctx,
                ProviderKind::VertexExpress,
                credential.key.as_str(),
                &request_model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::XGoogApiKey,
                credential.key.as_str(),
                |_| Ok(()),
                &gemini_request,
            )
            .await?;

            let parsed = parse_sse_response::<StreamGenerateContentResponse>(res).await?;
            let response_id = format!("chatcmpl-{}", Uuid::new_v4().simple());
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.key.clone();
            let model_name = request_model.clone();
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
                let chunk = crate::formats::transform::gen_gemini_generate_to_openai_chat_stream::event(
                    event,
                    &request_model,
                    &response_id,
                )
                .map_err(|_| StatusCode::BAD_GATEWAY)?;

                Ok(chunk)
            });

            render_sse_response(mapped)
        } else {
            let path = format!(
                "{}/{}:generateContent",
                super::gemini::vertex_express_version_path(version),
                model_path
            );
            let mut url = build_url(&provider.setting.base_url, &path)?;
            apply_gemini_query(&mut url, &query, false);
            let res = send_json_request_with_status(
                ctx,
                ProviderKind::VertexExpress,
                credential.key.as_str(),
                &request_model,
                ctx.http_client(),
                url.as_str(),
                &headers,
                AuthMode::XGoogApiKey,
                credential.key.as_str(),
                |_| Ok(()),
                &gemini_request,
            )
            .await?;

            let parsed = parse_json_response::<GenerateContentResponse>(res).await?;
            let mapped = transform::map_json_response(parsed, Ok)?;
            if let ParsedBody::Ok(ref response) = mapped.body
                && let Some(usage) = response.usage_metadata.clone() {
                    let record = build_gemini_usage_record(
                        ProviderKind::VertexExpress,
                        response.response_id.as_deref(),
                        &request_model,
                        caller_api_key.clone(),
                        credential.key.clone(),
                        usage,
                    );
                    let _ = ctx.usage_store().record(record).await;
                }

            let mapped = transform::map_json_response(mapped, |response| {
                crate::formats::transform::gen_gemini_generate_to_openai_chat::response(
                    response,
                    &request_model,
                )
                .map_err(|_| StatusCode::BAD_GATEWAY)
            })?;

            render_json_response(mapped)
        }
    }
}

#[async_trait::async_trait]
impl OpenAIResponses for VertexExpressProvider {
    async fn openai_responses(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::openai::responses::CreateResponseRequest>,
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
        _req: DownstreamRequest<crate::formats::openai::responses::CompactResponseRequest>,
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
impl OpenAIResponsesInputTokens for VertexExpressProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for VertexExpressProvider {
    async fn openai_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let list = load_models_value()?;
        json_response(&list)
    }
}

#[async_trait::async_trait]
impl OpenAIModelGet for VertexExpressProvider {
    async fn openai_model_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        _model: String,
    ) -> Result<Response, StatusCode> {
        let target = normalize_model_id(&_model);
        let list = load_models_value()?;
        let model = find_model_value(&list, &target).ok_or(StatusCode::NOT_FOUND)?;
        json_response(&model)
    }
}

#[async_trait::async_trait]
impl OpenAIConversations for VertexExpressProvider {
    async fn openai_conversations_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::openai::conversations::CreateConversationRequest>,
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
        _req: DownstreamRequest<crate::formats::openai::conversations::UpdateConversationRequest>,
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
impl OpenAIConversationItems for VertexExpressProvider {
    async fn openai_conversation_items_create(
        _ctx: &AppContext,
        _req: DownstreamRequest<crate::formats::openai::conversations::CreateConversationItemsRequest>,
        _conversation_id: String,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }

    async fn openai_conversation_items_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
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

const MODELS_JSON: &str = include_str!("models.openai.json");

fn load_models_value() -> Result<Value, StatusCode> {
    serde_json::from_str(MODELS_JSON).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn find_model_value(list: &Value, target: &str) -> Option<Value> {
    let data = list.get("data")?.as_array()?;
    data.iter()
        .find(|item| {
            item.get("id")
                .and_then(|value| value.as_str())
                .map(|id| normalize_model_id(id) == target)
                .unwrap_or(false)
        })
        .cloned()
}

fn normalize_model_id(model: &str) -> String {
    let model = model.trim_start_matches('/');
    model.strip_prefix("models/").unwrap_or(model).to_string()
}

fn json_response<T: serde::Serialize>(value: &T) -> Result<Response, StatusCode> {
    let body = serde_json::to_vec(value).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let mut response = Response::new(axum::body::Body::from(body));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}
