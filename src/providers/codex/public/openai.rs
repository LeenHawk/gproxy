use std::collections::HashMap;
use std::sync::OnceLock;

use futures_util::StreamExt;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use serde_json::Value;

use crate::context::AppContext;
use crate::formats::openai::chat_completions::CreateChatCompletionRequest;
use crate::formats::openai::conversations::{
    CreateConversationItemsRequest, CreateConversationRequest, UpdateConversationRequest,
};
use crate::formats::openai::responses::{
    CompactResponseRequest, CompactResponseResource, CreateResponseRequest, ResponseDeletedResource,
    ResponseItemList, ResponseObject, ResponseStreamEvent,
};
use crate::formats::openai::responses_input_tokens::{
    ResponseInputTokensRequest, ResponseInputTokensResponse, ResponseInputTokensObjectType,
};
use crate::providers::codex::CodexProvider;
use crate::providers::credential_status::ProviderKind;
use crate::providers::endpoints::{
    DownstreamRequest, OpenAIChatCompletions, OpenAIConversationItems, OpenAIConversations,
    OpenAIModelGet, OpenAIModelsList, OpenAIResponses, OpenAIResponsesInputTokens, UpstreamRequest,
};
use crate::providers::common::usage::build_openai_response_usage_record;
use crate::providers::router::{
    AuthMode, ParsedBody, ParsedJsonResponse, ParsedSseBody, ParsedSseResponse, SseMessage,
    apply_query, build_url, parse_json_response, parse_sse_response, render_json_response,
    render_sse_response, send_delete_request_with_status, send_get_request_with_status,
    send_json_request_with_status,
};
use crate::providers::codex::transform;
use crate::providers::router::not_implemented_response;
use crate::providers::codex::tokenizer::count_response_input_tokens;

#[async_trait::async_trait]
impl OpenAIChatCompletions for CodexProvider {
    async fn openai_chat_completions(
        _ctx: &AppContext,
        _req: DownstreamRequest<CreateChatCompletionRequest>,
    ) -> Result<Response, StatusCode> {
        Ok(not_implemented_response())
    }
}

#[async_trait::async_trait]
impl OpenAIResponses for CodexProvider {
    async fn openai_responses(
        ctx: &AppContext,
        req: DownstreamRequest<CreateResponseRequest>,
    ) -> Result<Response, StatusCode> {
        let caller_api_key = req.caller_api_key.clone();
        let UpstreamRequest {
            headers,
            query,
            mut body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let want_stream = body.stream.unwrap_or(false);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let allow_passthrough = is_codex_cli_user_agent(&headers);
        ensure_codex_request(&mut body, allow_passthrough)?;
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            body
                .model
                .as_deref()
                .unwrap_or(crate::providers::credential_status::DEFAULT_MODEL_KEY),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), true)?;
                Ok(())
            },
            &body,
        )
        .await?;
        let parsed = parse_sse_response::<ResponseStreamEvent>(res).await?;
        if want_stream {
            let usage_store = ctx.usage_store();
            let provider_credential_id = credential.account_id.clone();
            let mapped = transform::map_sse_response(parsed, move |event| {
                if let ResponseStreamEvent::ResponseCompleted { response, .. } = &event
                    && let Some(usage) = response.usage.clone() {
                        let record = build_openai_response_usage_record(
                            ProviderKind::Codex,
                            &response.id,
                            response.created_at,
                            &response.model,
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
            render_response_from_sse(parsed, ctx, caller_api_key, credential.account_id.clone())
                .await
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
        let stream = is_stream_query(&query);
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), stream)?;
                Ok(())
            },
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
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let res = send_delete_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), false)?;
                Ok(())
            },
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
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let empty_body = serde_json::Map::<String, serde_json::Value>::new();
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), false)?;
                Ok(())
            },
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
            mut body,
            path,
            ..
        } = transform::to_upstream_request(req);
        let allow_passthrough = is_codex_cli_user_agent(&headers);
        ensure_codex_instructions(&mut body.instructions, Some(body.model.as_str()), allow_passthrough)?;
        let provider = super::get_settings_and_credentials(ctx).await?;
        let credential = provider
            .pick_credential()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let res = send_json_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            body.model.as_str(),
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), false)?;
                Ok(())
            },
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
        let upstream_path = codex_path(&path);
        let mut url = build_url(&provider.setting.base_url, &upstream_path)?;
        apply_query(&mut url, &query);
        let res = send_get_request_with_status(
            ctx,
            ProviderKind::Codex,
            credential.account_id.as_str(),
            crate::providers::credential_status::DEFAULT_MODEL_KEY,
            ctx.http_client(),
            url.as_str(),
            &headers,
            AuthMode::AuthorizationBearer,
            credential.access_token.as_str(),
            |headers| {
                apply_codex_headers(headers, credential.account_id.as_str(), false)?;
                Ok(())
            },
        )
        .await?;
        let parsed = parse_json_response::<ResponseItemList>(res).await?;
        let mapped = transform::map_json_response(parsed, Ok)?;
        render_json_response(mapped)
    }
}

#[async_trait::async_trait]
impl OpenAIResponsesInputTokens for CodexProvider {
    async fn openai_responses_input_tokens(
        _ctx: &AppContext,
        req: DownstreamRequest<ResponseInputTokensRequest>,
    ) -> Result<Response, StatusCode> {
        let tokens = count_response_input_tokens(&req.body)?;
        let response = ResponseInputTokensResponse {
            object_type: ResponseInputTokensObjectType::ResponseInputTokens,
            input_tokens: tokens,
        };
        json_response(&response)
    }
}

#[async_trait::async_trait]
impl OpenAIModelsList for CodexProvider {
    async fn openai_models_list(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
    ) -> Result<Response, StatusCode> {
        let list = load_models_value()?;
        json_response(&list)
    }
}

#[async_trait::async_trait]
impl OpenAIModelGet for CodexProvider {
    async fn openai_model_get(
        _ctx: &AppContext,
        _req: DownstreamRequest<()>,
        model: String,
    ) -> Result<Response, StatusCode> {
        let target = normalize_model_id(&model);
        let list = load_models_value()?;
        let model = find_model_value(&list, &target).ok_or(StatusCode::NOT_FOUND)?;
        json_response(&model)
    }
}

#[async_trait::async_trait]
impl OpenAIConversations for CodexProvider {
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
impl OpenAIConversationItems for CodexProvider {
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
struct CodexInstructions {
    base_instructions: String,
    #[serde(default)]
    by_model: HashMap<String, String>,
}

fn load_instructions() -> Result<&'static CodexInstructions, StatusCode> {
    static CACHE: OnceLock<CodexInstructions> = OnceLock::new();
    if let Some(value) = CACHE.get() {
        return Ok(value);
    }
    let parsed: CodexInstructions = serde_json::from_str(include_str!("instructions.json"))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = CACHE.set(parsed);
    Ok(CACHE
        .get()
        .expect("instructions cache should be initialized"))
}

fn codex_path(path: &str) -> String {
    path.strip_prefix("/v1").unwrap_or(path).to_string()
}

fn is_stream_query(query: &HashMap<String, String>) -> bool {
    query
        .get("stream")
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false)
}

pub(crate) fn apply_codex_headers(
    headers: &mut axum::http::HeaderMap,
    account_id: &str,
    stream: bool,
) -> Result<(), StatusCode> {
    let account_id_value =
        HeaderValue::from_str(account_id).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert("chatgpt-account-id", account_id_value);
    let accept = if stream {
        HeaderValue::from_static("text/event-stream")
    } else {
        HeaderValue::from_static("application/json")
    };
    headers.insert(header::ACCEPT, accept);
    Ok(())
}

fn ensure_codex_request(
    body: &mut CreateResponseRequest,
    allow_passthrough: bool,
) -> Result<(), StatusCode> {
    body.stream = Some(true);
    body.store = Some(false);
    let model = body.model.as_deref();
    ensure_codex_instructions(&mut body.instructions, model, allow_passthrough)?;
    Ok(())
}

async fn render_response_from_sse(
    parsed: ParsedSseResponse<ResponseStreamEvent>,
    ctx: &AppContext,
    caller_api_key: Option<String>,
    provider_credential_id: String,
) -> Result<Response, StatusCode> {
    let ParsedSseResponse {
        status,
        headers,
        body,
    } = parsed;

    match body {
        ParsedSseBody::Error(value) => render_json_response(ParsedJsonResponse::<Value> {
            status,
            headers: json_headers(headers),
            body: ParsedBody::Error(value),
        }),
        ParsedSseBody::Stream(mut stream) => {
            while let Some(message) = stream.next().await {
                match message? {
                    SseMessage::Done => break,
                    SseMessage::Data(event) => match event {
                        ResponseStreamEvent::ResponseCompleted { response, .. } => {
                            if let Some(usage) = response.usage.clone() {
                                let record = build_openai_response_usage_record(
                                    ProviderKind::Codex,
                                    &response.id,
                                    response.created_at,
                                    &response.model,
                                    caller_api_key.clone(),
                                    provider_credential_id.clone(),
                                    usage,
                                );
                                let _ = ctx.usage_store().record(record).await;
                            }
                            return render_json_response(ParsedJsonResponse {
                                status,
                                headers: json_headers(headers),
                                body: ParsedBody::Ok(response),
                            });
                        }
                        ResponseStreamEvent::ResponseError { code, message, param, .. } => {
                            let error = serde_json::json!({
                                "error": {
                                    "code": code,
                                    "message": message,
                                    "param": param,
                                }
                            });
                            return render_json_response(ParsedJsonResponse::<Value> {
                                status: StatusCode::BAD_GATEWAY,
                                headers: json_headers(headers),
                                body: ParsedBody::Error(error),
                            });
                        }
                        _ => {}
                    },
                }
            }
            Err(StatusCode::BAD_GATEWAY)
        }
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

fn json_headers(mut headers: axum::http::HeaderMap) -> axum::http::HeaderMap {
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers
}

fn ensure_codex_instructions(
    instructions: &mut Option<String>,
    model: Option<&str>,
    allow_passthrough: bool,
) -> Result<(), StatusCode> {
    if allow_passthrough {
        return Ok(());
    }
    let data = load_instructions()?;
    let expected = model
        .and_then(|model| data.by_model.get(model))
        .unwrap_or(&data.base_instructions);
    let valid = instructions
        .as_ref()
        .map(|value| value.starts_with(&data.base_instructions))
        .unwrap_or(false);
    if !valid {
        *instructions = Some(expected.clone());
    }
    Ok(())
}

fn is_codex_cli_user_agent(headers: &HeaderMap) -> bool {
    let value = headers
        .get(header::USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    value.starts_with("codex_vscode/") || value.starts_with("codex_cli_rs/")
}
