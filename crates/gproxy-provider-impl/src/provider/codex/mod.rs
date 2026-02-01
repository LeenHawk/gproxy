use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{json, Value as JsonValue};
use tiktoken_rs::{CoreBPE, cl100k_base, get_bpe_from_model, o200k_base};

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, DownstreamContext, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta,
};
use gproxy_protocol::openai;
use openai::create_response::types::{
    EasyInputMessage, EasyInputMessageContent, EasyInputMessageRole, EasyInputMessageType, InputItem,
    Instructions, Metadata,
};
use gproxy_protocol::sse::SseParser;
use gproxy_transform::stream2nostream::openai_response::OpenAIResponseStreamToResponseState;

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::{
    dispatch_request, DispatchProvider, DispatchTable, TransformTarget, UsageKind, UpstreamOk,
    native_spec, transform_spec,
};
use crate::record::json_body_to_string;
use crate::storage::global_storage;
use crate::upstream::{handle_response, send_with_logging};
use crate::ProviderDefault;

pub const PROVIDER_NAME: &str = "codex";
const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const DISPATCH_TABLE: DispatchTable = DispatchTable::new([
    // Claude messages
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // Claude messages stream
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // Claude count tokens
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Claude models list
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Claude models get
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini generate
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // Gemini generate stream
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // Gemini count tokens
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini models list
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini models get
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // OpenAI chat
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // OpenAI chat stream
    transform_spec(TransformTarget::OpenAI, UsageKind::OpenAIResponses),
    // OpenAI responses
    native_spec(UsageKind::OpenAIResponses),
    // OpenAI responses stream
    native_spec(UsageKind::OpenAIResponses),
    // OpenAI input tokens
    native_spec(UsageKind::None),
    // OpenAI models list
    native_spec(UsageKind::None),
    // OpenAI models get
    native_spec(UsageKind::None),
    // Codex oauth start
    native_spec(UsageKind::None),
    // Codex oauth callback
    native_spec(UsageKind::None),
    // Codex usage
    native_spec(UsageKind::None),
]);

mod refresh;
mod oauth;
mod usage;
mod instructions;

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({ "base_url": DEFAULT_BASE_URL }),
        enabled: true,
    }
}

async fn channel_base_url(
    ctx: &UpstreamContext,
) -> Result<String, UpstreamPassthroughError> {
    let mut base_url = DEFAULT_BASE_URL.to_string();
    if let Some(storage) = global_storage() {
        let providers = storage
            .list_providers()
            .await
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let provider = if let Some(id) = ctx.provider_id {
            providers.iter().find(|provider| provider.id == id)
        } else {
            providers.iter().find(|provider| provider.name == PROVIDER_NAME)
        };
        if let Some(provider) = provider
            && let Some(map) = provider.config_json.as_object()
            && let Some(value) = map.get("base_url").and_then(|v| v.as_str())
        {
            base_url = value.to_string();
        }
    }
    Ok(base_url.trim_end_matches('/').to_string())
}

#[derive(Debug)]
pub struct CodexProvider {
    pool: CredentialPool<CodexCredential>,
}

pub type CodexCredential = BaseCredential;

impl CodexProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<CodexCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<CodexCredential>) {
        self.pool.replace_snapshot(snapshot);
    }

    pub async fn fetch_usage_payload(
        &self,
        ctx: UpstreamContext,
    ) -> Result<JsonValue, UpstreamPassthroughError> {
        usage::fetch_usage_payload(&self.pool, ctx).await
    }

    pub async fn fetch_usage_payload_for_credential(
        &self,
        credential_id: i64,
        ctx: UpstreamContext,
    ) -> Result<JsonValue, UpstreamPassthroughError> {
        usage::fetch_usage_payload_for_credential(&self.pool, ctx, credential_id).await
    }
}

#[async_trait]
impl Provider for CodexProvider {
    fn name(&self) -> &str {
        PROVIDER_NAME
    }

    async fn call(
        &self,
        req: ProxyRequest,
        ctx: DownstreamContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError> {
        dispatch_request(self, req, ctx).await
    }
}

#[async_trait]
impl DispatchProvider for CodexProvider {
    fn dispatch_table(&self) -> &'static DispatchTable {
        &DISPATCH_TABLE
    }

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        match req {
            ProxyRequest::OpenAIResponses(request) => self.handle_responses(request, false, ctx).await,
            ProxyRequest::OpenAIResponsesStream(request) => {
                self.handle_responses(request, true, ctx).await
            }
            ProxyRequest::OpenAIInputTokens(request) => self.handle_input_tokens(request, ctx).await,
            ProxyRequest::OpenAIModelsList(request) => self.handle_models_list(request, ctx).await,
            ProxyRequest::OpenAIModelsGet(request) => self.handle_models_get(request, ctx).await,
            ProxyRequest::OAuthStart { query, headers } => {
                oauth::handle_oauth_start(query, headers, ctx).await
            }
            ProxyRequest::OAuthCallback { query, headers } => {
                oauth::handle_oauth_callback(&self.pool, query, headers, ctx).await
            }
            ProxyRequest::Usage => usage::handle_usage(&self.pool, ctx).await,
            _ => Err(UpstreamPassthroughError::service_unavailable(
                "non-native operation".to_string(),
            )),
        }
    }
}

impl CodexProvider {
    async fn handle_responses(
        &self,
        request: openai::create_response::request::CreateResponseRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.body.model.clone();
        let scope = DisallowScope::model(model.clone());
        let mut body = request.body;
        body.stream = Some(true);
        body.store = Some(false);
        // Codex backend does not accept max_output_tokens; strip it to avoid 400s.
        body.max_output_tokens = None;
        let is_codex_ua = is_codex_user_agent(ctx.user_agent.as_deref());
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let mut body = body.clone();
                let base_url = base_url.clone();
                async move {
                    if !is_codex_ua {
                        let personality =
                            resolve_non_codex_personality(body.metadata.as_ref(), credential.value());
                        let mut extra = instructions::instructions_for_model(&model, personality);
                        if let Some(custom) = credential_non_codex_instructions(credential.value())
                            && !custom.trim().is_empty() {
                                extra = format!("{extra}\n\n{custom}");
                            }
                        apply_non_codex_instructions(&mut body, &extra);
                    }
                    let tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let mut access_token = tokens.access_token.clone();
                    let refresh_token = tokens
                        .refresh_token
                        .clone()
                        .or_else(|| credential_refresh_token(credential.value()));
                    let account_id = credential_account_id(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing account_id"))?;
                    let path = "/responses".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let url_req = url.clone();
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let mut req_headers = build_codex_headers(&access_token, &account_id)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let mut response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "codex.responses",
                        "POST",
                        &path,
                        Some(&model),
                        true,
                        &scope,
                        || {
                            client
                                .post(url_req)
                                .headers(req_headers.clone())
                                .json(&body)
                                .send()
                        },
                    )
                    .await?;
                    if (response.status() == StatusCode::UNAUTHORIZED
                        || response.status() == StatusCode::FORBIDDEN)
                        && let Some(refresh_token) = refresh_token {
                    let refresh_url = refresh::refresh_token_url(&ctx)
                        .await
                        .unwrap_or_else(|_| "https://auth.openai.com/oauth/token".to_string());
                    let refreshed = refresh::refresh_access_token(
                        credential.value().id,
                        refresh_token,
                        &refresh_url,
                        &ctx,
                        &scope,
                    )
                    .await?;
                            access_token = refreshed.access_token;
                            req_headers = build_codex_headers(&access_token, &account_id)?;
                            response = send_with_logging(
                                &ctx,
                                PROVIDER_NAME,
                                "codex.responses",
                                "POST",
                                &path,
                                Some(&model),
                                true,
                                &scope,
                                || {
                                    client
                                        .post(url.clone())
                                        .headers(req_headers.clone())
                                        .json(&body)
                                        .send()
                                },
                            )
                            .await?;
                        }
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "codex.responses".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response = handle_response(
                        response,
                        true,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    let response = if is_stream {
                        response
                    } else {
                        stream_to_openai_response(response)
                            .await
                            .map_err(passthrough_failure)?
                    };
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_input_tokens(
        &self,
        request: openai::count_tokens::request::InputTokenCountRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.body.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                async move {
                    let _tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = "{}".to_string();
                    let token_count = count_input_tokens(&body).map_err(passthrough_failure)?;
                    let response_body = openai::count_tokens::response::InputTokenCountResponse {
                        object: openai::count_tokens::types::InputTokenObjectType::ResponseInputTokens,
                        input_tokens: token_count,
                    };
                    let body_bytes = serde_json::to_vec(&response_body).map_err(|err| {
                        AttemptFailure {
                            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                            mark: None,
                        }
                    })?;
                    let mut headers = HeaderMap::new();
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    let response = ProxyResponse::Json {
                        status: StatusCode::OK,
                        headers: headers.clone(),
                        body: Bytes::from(body_bytes),
                    };
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "codex.input_tokens".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: "/v1/responses/input_tokens".to_string(),
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_list(
        &self,
        _request: openai::list_models::request::ListModelsRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                async move {
                    let _tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let list = load_models_value().map_err(passthrough_failure)?;
                    let body_bytes = serde_json::to_vec(list).map_err(|err| AttemptFailure {
                        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                        mark: None,
                    })?;
                    let mut headers = HeaderMap::new();
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    let response = ProxyResponse::Json {
                        status: StatusCode::OK,
                        headers: headers.clone(),
                        body: Bytes::from(body_bytes),
                    };
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "codex.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: "/v1/models".to_string(),
                        request_query: None,
                        request_headers: "{}".to_string(),
                        request_body: String::new(),
                    };
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_get(
        &self,
        request: openai::get_model::request::GetModelRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = normalize_model_id(&request.path.model);
        let scope = DisallowScope::model(model.clone());

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                async move {
                    let _tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let list = load_models_value().map_err(passthrough_failure)?;
                    let model_value = find_model_value(list, &model)
                        .ok_or_else(|| passthrough_failure(UpstreamPassthroughError::from_status(
                            StatusCode::NOT_FOUND,
                            "model not found",
                        )))?;
                    let body_bytes = serde_json::to_vec(&model_value).map_err(|err| AttemptFailure {
                        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                        mark: None,
                    })?;
                    let mut headers = HeaderMap::new();
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    let response = ProxyResponse::Json {
                        status: StatusCode::OK,
                        headers: headers.clone(),
                        body: Bytes::from(body_bytes),
                    };
                    let request_path = format!("/v1/models/{model}");
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "codex.models_get".to_string(),
                        model: Some(model.clone()),
                        request_method: "GET".to_string(),
                        request_path,
                        request_query: None,
                        request_headers: "{}".to_string(),
                        request_body: String::new(),
                    };
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }
}

async fn stream_to_openai_response(
    response: ProxyResponse,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match response {
        ProxyResponse::Stream { status, mut headers, body } => {
            let mut parser = SseParser::new();
            let mut state = OpenAIResponseStreamToResponseState::new();
            let mut stream = body.stream;
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|err| {
                    UpstreamPassthroughError::service_unavailable(err.to_string())
                })?;
                for event in parser.push_bytes(&bytes) {
                    if event.data.is_empty() || event.data == "[DONE]" {
                        continue;
                    }
                    let parsed = serde_json::from_str::<
                        openai::create_response::stream::ResponseStreamEvent,
                    >(&event.data)
                    .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
                    if let openai::create_response::stream::ResponseStreamEvent::Error(err) = &parsed {
                        return Err(UpstreamPassthroughError::service_unavailable(
                            err.message.clone(),
                        ));
                    }
                    if let Some(response) = state.push_event(parsed) {
                        let body = serde_json::to_vec(&response)
                            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
                        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                        headers.remove(http::header::TRANSFER_ENCODING);
                        headers.remove(http::header::CONTENT_LENGTH);
                        return Ok(ProxyResponse::Json {
                            status,
                            headers,
                            body: Bytes::from(body),
                        });
                    }
                }
            }
            for event in parser.finish() {
                if event.data.is_empty() || event.data == "[DONE]" {
                    continue;
                }
                let parsed = serde_json::from_str::<
                    openai::create_response::stream::ResponseStreamEvent,
                >(&event.data)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
                if let openai::create_response::stream::ResponseStreamEvent::Error(err) = &parsed {
                    return Err(UpstreamPassthroughError::service_unavailable(
                        err.message.clone(),
                    ));
                }
                if let Some(response) = state.push_event(parsed) {
                    let body = serde_json::to_vec(&response)
                        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    headers.remove(http::header::TRANSFER_ENCODING);
                    headers.remove(http::header::CONTENT_LENGTH);
                    return Ok(ProxyResponse::Json {
                        status,
                        headers,
                        body: Bytes::from(body),
                    });
                }
            }
            Err(UpstreamPassthroughError::service_unavailable(
                "missing response.completed event".to_string(),
            ))
        }
        ProxyResponse::Json { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected stream response".to_string(),
        )),
    }
}

fn credential_access_token(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("access_token")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_refresh_token(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_account_id(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("account_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_non_codex_instructions(credential: &BaseCredential) -> Option<String> {
    credential
        .meta
        .get("non_codex_instructions")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn apply_non_codex_instructions(
    body: &mut openai::create_response::request::CreateResponseRequestBody,
    extra: &str,
) {
    let extra = extra.trim();
    if extra.is_empty() {
        return;
    }
    let extra_text = extra.to_string();
    body.instructions = match body.instructions.take() {
        Some(Instructions::Text(existing)) => {
            if existing.trim().is_empty() {
                Some(Instructions::Text(extra_text))
            } else {
                Some(Instructions::Text(format!("{existing}\n\n{extra}")))
            }
        }
        Some(Instructions::Items(mut items)) => {
            items.push(instruction_text_item(extra_text));
            Some(Instructions::Items(items))
        }
        None => Some(Instructions::Text(extra_text)),
    };
}

fn instruction_text_item(text: String) -> InputItem {
    InputItem::EasyMessage(EasyInputMessage {
        r#type: EasyInputMessageType::Message,
        role: EasyInputMessageRole::System,
        content: EasyInputMessageContent::Text(text),
    })
}

fn resolve_non_codex_personality(
    metadata: Option<&Metadata>,
    credential: &BaseCredential,
) -> Option<instructions::CodexPersonality> {
    metadata
        .and_then(|meta| {
            meta.get("codex_personality")
                .or_else(|| meta.get("personality"))
                .and_then(|value| instructions::parse_personality(value))
        })
        .or_else(|| credential_personality(credential))
}

fn credential_personality(credential: &BaseCredential) -> Option<instructions::CodexPersonality> {
    credential
        .meta
        .get("codex_personality")
        .or_else(|| credential.meta.get("personality"))
        .and_then(|value| value.as_str())
        .and_then(instructions::parse_personality)
}

fn is_codex_user_agent(user_agent: Option<&str>) -> bool {
    user_agent
        .map(|ua| ua.to_ascii_lowercase().contains("codex"))
        .unwrap_or(false)
}


#[allow(clippy::result_large_err)]
fn build_codex_headers(
    access_token: &str,
    account_id: &str,
) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    let auth_value = HeaderValue::from_str(&format!("Bearer {access_token}"))
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let account_value = HeaderValue::from_str(account_id).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    headers.insert(AUTHORIZATION, auth_value);
    headers.insert("chatgpt-account-id", account_value);
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("text/event-stream"));
    Ok(headers)
}

#[allow(clippy::result_large_err)]
fn build_codex_json_headers(
    access_token: &str,
    account_id: &str,
) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = build_codex_headers(access_token, account_id)?;
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn build_url(base_url: Option<&str>, path: &str) -> String {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let mut path = path.trim_start_matches('/');
    if base.ends_with("/v1") && (path == "v1" || path.starts_with("v1/")) {
        path = path.trim_start_matches("v1/").trim_start_matches("v1");
    }
    format!("{base}/{path}")
}

fn build_usage_url(base_url: Option<&str>) -> (String, String) {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let base = base.strip_suffix("/codex").unwrap_or(base);
    (format!("{base}/wham/usage"), "wham/usage".to_string())
}

fn invalid_credential(scope: &DisallowScope, message: &str) -> AttemptFailure {
    AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(message.to_string()),
        mark: Some(gproxy_provider_core::DisallowMark {
            scope: scope.clone(),
            level: gproxy_provider_core::DisallowLevel::Dead,
            duration: None,
            reason: Some(message.to_string()),
        }),
    }
}

fn passthrough_failure(err: UpstreamPassthroughError) -> AttemptFailure {
    AttemptFailure {
        passthrough: err,
        mark: None,
    }
}

#[allow(clippy::result_large_err)]
fn count_input_tokens(body: &openai::count_tokens::request::InputTokenCountRequestBody) -> Result<i64, UpstreamPassthroughError> {
    let bpe = bpe_for_model(Some(&body.model))
        .map_err(UpstreamPassthroughError::service_unavailable)?;
    let mut total = 0i64;

    if let Some(input) = &body.input {
        total += count_input_param(input, &bpe);
    }
    if let Some(instructions) = &body.instructions {
        total += count_text(instructions, &bpe);
    }

    Ok(total)
}

fn bpe_for_model(model: Option<&str>) -> Result<CoreBPE, String> {
    if let Some(model) = model {
        if let Ok(bpe) = get_bpe_from_model(model) {
            return Ok(bpe);
        }
        if is_o200k_model(model) {
            return o200k_base().map_err(|err| err.to_string());
        }
    }
    cl100k_base().map_err(|err| err.to_string())
}

fn is_o200k_model(model: &str) -> bool {
    model.starts_with("gpt-5")
        || model.starts_with("gpt-4.1")
        || model.starts_with("gpt-4o")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
}

fn count_input_param(input: &openai::create_response::types::InputParam, bpe: &CoreBPE) -> i64 {
    match input {
        openai::create_response::types::InputParam::Text(text) => count_text(text, bpe),
        openai::create_response::types::InputParam::Items(items) => {
            items.iter().map(|item| count_input_item(item, bpe)).sum()
        }
    }
}

fn count_input_item(item: &openai::create_response::types::InputItem, bpe: &CoreBPE) -> i64 {
    use openai::create_response::types::InputItem;
    match item {
        InputItem::EasyMessage(message) => count_easy_message(&message.content, bpe),
        InputItem::Reference(_) => 0,
        InputItem::Item(item) => count_item(item, bpe),
    }
}

fn count_easy_message(
    content: &openai::create_response::types::EasyInputMessageContent,
    bpe: &CoreBPE,
) -> i64 {
    match content {
        openai::create_response::types::EasyInputMessageContent::Text(text) => count_text(text, bpe),
        openai::create_response::types::EasyInputMessageContent::Parts(parts) => {
            parts.iter().map(|part| count_input_content(part, bpe)).sum()
        }
    }
}

fn count_item(item: &openai::create_response::types::Item, bpe: &CoreBPE) -> i64 {
    use openai::create_response::types::Item;
    match item {
        Item::InputMessage(message) => count_input_message(message, bpe),
        Item::OutputMessage(message) => count_output_message(message, bpe),
        Item::FunctionOutput(output) => count_tool_call_output(&output.output, bpe),
        Item::CustomToolCallOutput(output) => count_tool_call_output(&output.output, bpe),
        _ => 0,
    }
}

fn count_input_message(message: &openai::create_response::types::InputMessage, bpe: &CoreBPE) -> i64 {
    message
        .content
        .iter()
        .map(|part| count_input_content(part, bpe))
        .sum()
}

fn count_output_message(
    message: &openai::create_response::types::OutputMessage,
    bpe: &CoreBPE,
) -> i64 {
    use openai::create_response::types::OutputMessageContent;
    message
        .content
        .iter()
        .map(|part| match part {
            OutputMessageContent::OutputText(text) => count_text(&text.text, bpe),
            OutputMessageContent::Refusal(refusal) => count_text(&refusal.refusal, bpe),
        })
        .sum()
}

fn count_tool_call_output(
    output: &openai::create_response::types::ToolCallOutput,
    bpe: &CoreBPE,
) -> i64 {
    use openai::create_response::types::{FunctionAndCustomToolCallOutput, ToolCallOutput};
    match output {
        ToolCallOutput::Text(text) => count_text(text, bpe),
        ToolCallOutput::Content(parts) => parts
            .iter()
            .map(|part| match part {
                FunctionAndCustomToolCallOutput::InputText(content) => count_text(&content.text, bpe),
                FunctionAndCustomToolCallOutput::InputImage(_) => 0,
                FunctionAndCustomToolCallOutput::InputFile(_) => 0,
            })
            .sum(),
    }
}

fn count_input_content(
    content: &openai::create_response::types::InputContent,
    bpe: &CoreBPE,
) -> i64 {
    match content {
        openai::create_response::types::InputContent::InputText(text) => count_text(&text.text, bpe),
        openai::create_response::types::InputContent::InputImage(_) => 0,
        openai::create_response::types::InputContent::InputFile(_) => 0,
    }
}

fn count_text(text: &str, bpe: &CoreBPE) -> i64 {
    bpe.encode_ordinary(text).len() as i64
}

static MODELS_CACHE: OnceLock<JsonValue> = OnceLock::new();

#[allow(clippy::result_large_err)]
fn load_models_value() -> Result<&'static JsonValue, UpstreamPassthroughError> {
    if let Some(value) = MODELS_CACHE.get() {
        return Ok(value);
    }
    let raw = include_str!("models.json");
    let parsed: JsonValue = serde_json::from_str(raw)
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    if parsed.get("data").and_then(|v| v.as_array()).is_none() {
        return Err(UpstreamPassthroughError::service_unavailable(
            "models.json missing data array".to_string(),
        ));
    }
    let _ = MODELS_CACHE.set(parsed);
    Ok(MODELS_CACHE.get().expect("models cache initialized"))
}

fn find_model_value(list: &JsonValue, target: &str) -> Option<JsonValue> {
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

fn headers_to_json(headers: &HeaderMap) -> String {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        if let Ok(value) = value.to_str() {
            map.insert(name.to_string(), serde_json::Value::String(value.to_string()));
        }
    }
    serde_json::Value::Object(map).to_string()
}
