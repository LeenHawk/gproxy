use std::sync::Arc;

use async_trait::async_trait;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{json, Value as JsonValue};

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, DownstreamContext, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta,
};
use gproxy_protocol::openai;

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::{
    dispatch_request, DispatchProvider, DispatchTable, TransformTarget, UsageKind, UpstreamOk,
    native_spec, transform_spec, unsupported_spec,
};
use crate::record::{headers_to_json, json_body_to_string};
use crate::storage::global_storage;
use crate::upstream::{handle_response, send_with_logging};
use crate::ProviderDefault;

mod tokenizer;

pub const PROVIDER_NAME: &str = "nvidia";
const DEFAULT_BASE_URL: &str = "https://integrate.api.nvidia.com";
const DISPATCH_TABLE: DispatchTable = DispatchTable::new([
    // Claude messages
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // Claude messages stream
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // Claude count tokens
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Claude models list
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Claude models get
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini generate
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // Gemini generate stream
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // Gemini count tokens
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini models list
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // Gemini models get
    transform_spec(TransformTarget::OpenAI, UsageKind::None),
    // OpenAI chat
    native_spec(UsageKind::OpenAIChat),
    // OpenAI chat stream
    native_spec(UsageKind::OpenAIChat),
    // OpenAI responses
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // OpenAI responses stream
    transform_spec(TransformTarget::OpenAIChat, UsageKind::OpenAIChat),
    // OpenAI input tokens (local tokenizer)
    native_spec(UsageKind::None),
    // OpenAI models list
    native_spec(UsageKind::None),
    // OpenAI models get
    native_spec(UsageKind::None),

    // Codex oauth start
    unsupported_spec(),
    // Codex oauth callback
    unsupported_spec(),
    // Codex usage
    unsupported_spec(),
]);

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({
            "base_url": DEFAULT_BASE_URL,
            "hf_token": "",
            "hf_url": "https://huggingface.co",
            "data_dir": "./data"
        }),
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
                && let Some(value) = map.get("base_url").and_then(|v| v.as_str()) {
                    base_url = value.to_string();
                }
    }
    Ok(base_url.trim_end_matches('/').to_string())
}

#[derive(Debug)]
pub struct NvidiaProvider {
    pool: CredentialPool<NvidiaCredential>,
}

pub type NvidiaCredential = BaseCredential;

impl NvidiaProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<NvidiaCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<NvidiaCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for NvidiaProvider {
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
impl DispatchProvider for NvidiaProvider {
    fn dispatch_table(&self) -> &'static DispatchTable {
        &DISPATCH_TABLE
    }

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        match req {
            ProxyRequest::OpenAIChat(request) => self.handle_chat(request, false, ctx).await,
            ProxyRequest::OpenAIChatStream(request) => self.handle_chat(request, true, ctx).await,
            ProxyRequest::OpenAIInputTokens(request) => self.handle_input_tokens(request, ctx).await,
            ProxyRequest::OpenAIModelsList(request) => self.handle_models_list(request, ctx).await,
            ProxyRequest::OpenAIModelsGet(request) => self.handle_models_get(request, ctx).await,
            _ => Err(UpstreamPassthroughError::service_unavailable(
                "non-native operation".to_string(),
            )),
        }
    }
}

impl NvidiaProvider {
    async fn handle_chat(
        &self,
        request: openai::create_chat_completions::request::CreateChatCompletionRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
    let model = request.body.model.clone();
        let scope = DisallowScope::model(model.clone());
        let mut body = request.body;
        if is_stream {
            body.stream = Some(true);
            match &mut body.stream_options {
                Some(options) => {
                    if options.include_usage.is_none() {
                        options.include_usage = Some(true);
                    }
                }
                None => {
                    body.stream_options = Some(
                        openai::create_chat_completions::types::ChatCompletionStreamOptions {
                            include_usage: Some(true),
                            include_obfuscation: None,
                        },
                    );
                }
            }
        }
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let path = "/v1/chat/completions".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "nvidia.chat.completions",
                        "POST",
                        &path,
                        Some(&model),
                        is_stream,
                        &scope,
                        || {
                            client
                                .post(url)
                                .headers(req_headers.clone())
                                .json(&body)
                                .send()
                        },
                    )
                    .await?;
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "nvidia.chat.completions".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response =
                        handle_response(response, is_stream, scope.clone(), &ctx, Some(meta.clone()))
                            .await?;
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
                    let _api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let hf_token = credential_hf_token(credential.value());
                    let hf_url = credential_hf_url(credential.value());
                    let data_dir = credential_data_dir(credential.value());
                    let request_body = json_body_to_string(&body);
                    let request_headers = "{}".to_string();
                    let token_count = self::tokenizer::count_input_tokens(
                        &model,
                        &body,
                        ctx.proxy.as_deref(),
                        hf_token.as_deref(),
                        hf_url.as_deref(),
                        data_dir.as_deref(),
                    )
                    .await?;
                    let response_body = openai::count_tokens::response::InputTokenCountResponse {
                        object: openai::count_tokens::types::InputTokenObjectType::ResponseInputTokens,
                        input_tokens: token_count,
                    };
                    let body_bytes = serde_json::to_vec(&response_body).map_err(|err| AttemptFailure {
                        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                        mark: None,
                    })?;
                    let mut headers = HeaderMap::new();
                    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                    let response = ProxyResponse::Json {
                        status: StatusCode::OK,
                        headers: headers.clone(),
                        body: bytes::Bytes::from(body_bytes),
                    };
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "nvidia.input_tokens".to_string(),
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
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let path = "/v1/models".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "nvidia.models_list",
                        "GET",
                        &path,
                        None,
                        false,
                        &scope,
                        || client.get(url).headers(req_headers.clone()).send(),
                    )
                    .await?;
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "nvidia.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body: String::new(),
                    };
                    let response =
                        handle_response(response, false, scope.clone(), &ctx, Some(meta.clone()))
                            .await?;
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
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let path = format!("/v1/models/{model}");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "nvidia.models_get",
                        "GET",
                        &path,
                        Some(&model),
                        false,
                        &scope,
                        || client.get(url).headers(req_headers.clone()).send(),
                    )
                    .await?;
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "nvidia.models_get".to_string(),
                        model: Some(model),
                        request_method: "GET".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body: String::new(),
                    };
                    let response =
                        handle_response(response, false, scope.clone(), &ctx, Some(meta.clone()))
                            .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }
}

#[allow(clippy::result_large_err)]
fn build_openai_headers(api_key: &str) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    let mut bearer = String::with_capacity(api_key.len() + 7);
    bearer.push_str("Bearer ");
    bearer.push_str(api_key);
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&bearer).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

fn credential_api_key(credential: &BaseCredential) -> Option<String> {
    if let JsonValue::String(value) = &credential.secret {
        return Some(value.clone());
    }
    credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_hf_token(credential: &BaseCredential) -> Option<String> {
    credential
        .meta
        .get("hf_token")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_hf_url(credential: &BaseCredential) -> Option<String> {
    credential
        .meta
        .get("hf_url")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_data_dir(credential: &BaseCredential) -> Option<String> {
    credential
        .meta
        .get("data_dir")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn build_url(base_url: Option<&str>, path: &str) -> String {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let mut path = path.trim_start_matches('/');
    if base.ends_with("/v1") && (path == "v1" || path.starts_with("v1/")) {
        path = path.trim_start_matches("v1/").trim_start_matches("v1");
    }
    format!("{base}/{path}")
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
