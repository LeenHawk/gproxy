use async_stream::stream;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::Response;
use futures_util::StreamExt;
use serde_json::Value;

use crate::context::AppContext;
use crate::formats::claude::messages::MessageCreateResponse;
use crate::formats::claude::stream::ClaudeStreamEvent;
use crate::formats::openai::chat_completions::CreateChatCompletionRequest;
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::responses::{CompactResponseRequest, CreateResponseRequest};
use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;
use crate::formats::transform::{
    RequestParts as TransformRequestParts, ResponseParts as TransformResponseParts,
};
use crate::formats::transform::{
    gen_claude_messages_to_openai_chat, gen_claude_messages_to_openai_chat_stream,
    gen_openai_chat_to_claude_messages,
};
use crate::providers::claudecode::{
    ClaudeCodeProvider, CLAUDE_API_VERSION, CLAUDE_BETA_BASE, CLAUDE_CODE_USER_AGENT,
};
use crate::providers::claudecode::transform as provider_transform;
use crate::providers::common::usage::{
    build_claude_stream_usage_record, build_claude_usage_record, ensure_openai_stream_usage,
};
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, OpenAIChatCompletions, OpenAIConversationItems, OpenAIConversations,
    OpenAIModelGet, OpenAIModelsList, OpenAIResponses, OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::router::{
    AuthMode, ParsedBody, ParsedJsonResponse, ParsedSseBody, ParsedSseResponse, SseMessage,
    apply_query, build_url, not_implemented_response, parse_json_response, parse_sse_response,
    render_json_response, render_sse_response, send_json_request_with_status,
};

#[async_trait::async_trait]
impl OpenAIChatCompletions for ClaudeCodeProvider {
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
        } = provider_transform::to_upstream_request(req);

        let mut body = body;
        ensure_openai_stream_usage(&mut body);

        let mut claude_req = gen_openai_chat_to_claude_messages::request(TransformRequestParts {
            path,
            query,
            headers,
            body,
        })
        .map_err(|_| StatusCode::BAD_REQUEST)?;

        claude_req.body.system =
            super::claude::normalize_system_prompt(claude_req.body.system.take());

        let provider = super::get_settings_and_credentials(ctx).await?;
        let model = claude_req.body.model.as_str().to_string();
        let credential = provider
            .pick_credential_for_model(&model)
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut url = build_url(&provider.setting.base_url, &claude_req.path)?;
        apply_query(&mut url, &claude_req.query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::ClaudeCode,
            credential.refresh_token.as_str(),
            model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &claude_req.headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |out_headers| {
                out_headers.insert(
                    "anthropic-version",
                    HeaderValue::from_static(CLAUDE_API_VERSION),
                );
                out_headers.insert(
                    "anthropic-beta",
                    HeaderValue::from_static(CLAUDE_BETA_BASE),
                );
                out_headers.insert(
                    header::USER_AGENT,
                    HeaderValue::from_static(CLAUDE_CODE_USER_AGENT),
                );
                Ok(())
            },
            &claude_req.body,
        )
        .await?;

        if claude_req.body.stream == Some(true) {
            let parsed = parse_sse_response::<ClaudeStreamEvent>(res).await?;
            let ParsedSseResponse {
                status,
                headers,
                body,
            } = parsed;
            let mapped_body = match body {
                ParsedSseBody::Error(value) => ParsedSseBody::Error(value),
                ParsedSseBody::Stream(mut stream) => {
                    let usage_store = ctx.usage_store();
                    let provider_credential_id = credential.refresh_token.clone();
                    let mut recorded = false;
                    let mut message_id: Option<String> = None;
                    let mut message_model: Option<String> = None;
                    let mut state = gen_claude_messages_to_openai_chat_stream::StreamState::new();
                    let request_model = model.clone();

                    let output_stream = stream! {
                        while let Some(item) = stream.next().await {
                            match item {
                                Ok(SseMessage::Data(event)) => {
                                    match &event {
                                        ClaudeStreamEvent::MessageStart { message } => {
                                            message_id = Some(message.id.clone());
                                            message_model = Some(message.model.as_str().to_string());
                                            if !recorded
                                                && let Some(usage) = message.usage.clone() {
                                                    let record = build_claude_stream_usage_record(
                                                        ProviderKind::ClaudeCode,
                                                        message_id.as_deref(),
                                                        message_model
                                                            .as_deref()
                                                            .unwrap_or(request_model.as_str()),
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
                                        ClaudeStreamEvent::MessageDelta {
                                            usage: Some(usage),
                                            ..
                                        } => {
                                            if !recorded {
                                                let record = build_claude_stream_usage_record(
                                                    ProviderKind::ClaudeCode,
                                                    message_id.as_deref(),
                                                    message_model
                                                        .as_deref()
                                                        .unwrap_or(request_model.as_str()),
                                                    caller_api_key.clone(),
                                                    provider_credential_id.clone(),
                                                    usage.clone(),
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

                                    match state.map_event(event) {
                                        Ok(Some(chunk)) => {
                                            yield Ok(SseMessage::Data(chunk));
                                        }
                                        Ok(None) => {}
                                        Err(_) => {
                                            yield Err(StatusCode::BAD_GATEWAY);
                                            return;
                                        }
                                    }
                                }
                                Ok(SseMessage::Done) => {
                                    yield Ok(SseMessage::Done);
                                    return;
                                }
                                Err(status) => {
                                    yield Err(status);
                                    return;
                                }
                            }
                        }
                    };

                    ParsedSseBody::Stream(Box::pin(output_stream))
                }
            };

            render_sse_response(ParsedSseResponse {
                status,
                headers,
                body: mapped_body,
            })
        } else {
            let parsed = parse_json_response::<MessageCreateResponse>(res).await?;
            let ParsedJsonResponse {
                status,
                headers,
                body,
            } = parsed;
            match body {
                ParsedBody::Ok(body) => {
                    let record = build_claude_usage_record(
                        ProviderKind::ClaudeCode,
                        Some(body.id.as_str()),
                        body.model.as_str(),
                        caller_api_key.clone(),
                        credential.refresh_token.clone(),
                        body.usage.clone(),
                    );
                    let _ = ctx.usage_store().record(record).await;

                    let mapped =
                        gen_claude_messages_to_openai_chat::response(TransformResponseParts {
                            status,
                            headers,
                            body,
                        })
                        .map_err(|_| StatusCode::BAD_GATEWAY)?;
                    render_json_response(ParsedJsonResponse {
                        status: mapped.status,
                        headers: mapped.headers,
                        body: ParsedBody::Ok(mapped.body),
                    })
                }
                ParsedBody::Error(value) => render_json_response(ParsedJsonResponse::<Value> {
                    status,
                    headers,
                    body: ParsedBody::Error(value),
                }),
            }
        }
    }
}

#[async_trait::async_trait]
impl OpenAIResponses for ClaudeCodeProvider {
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
impl OpenAIResponsesInputTokens for ClaudeCodeProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        _req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for ClaudeCodeProvider {
    async fn openai_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let list = load_models_value()?;
        json_response(&list)
    }
}

#[async_trait::async_trait]
impl OpenAIModelGet for ClaudeCodeProvider {
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
impl OpenAIConversations for ClaudeCodeProvider {
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
impl OpenAIConversationItems for ClaudeCodeProvider {
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
