use std::sync::Arc;
use async_trait::async_trait;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue};
use serde_json::json;

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, DownstreamContext, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta,
};
use gproxy_protocol::{gemini, openai};

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
use crate::provider::not_implemented;

pub const PROVIDER_NAME: &str = "aistudio";
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const DISPATCH_TABLE: DispatchTable = DispatchTable::new([
    // Claude messages
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
    // Claude messages stream
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
    // Claude count tokens
    transform_spec(TransformTarget::Gemini, UsageKind::None),
    // Claude models list
    transform_spec(TransformTarget::Gemini, UsageKind::None),
    // Claude models get
    transform_spec(TransformTarget::Gemini, UsageKind::None),
    // Gemini generate
    native_spec(UsageKind::GeminiGenerate),
    // Gemini generate stream
    native_spec(UsageKind::GeminiGenerate),
    // Gemini count tokens
    native_spec(UsageKind::None),
    // Gemini models list
    native_spec(UsageKind::None),
    // Gemini models get
    native_spec(UsageKind::None),
    // OpenAI chat
    native_spec(UsageKind::OpenAIChat),
    // OpenAI chat stream
    native_spec(UsageKind::OpenAIChat),
    // OpenAI responses
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
    // OpenAI responses stream
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
    // OpenAI input tokens
    transform_spec(TransformTarget::Gemini, UsageKind::None),
    // OpenAI models list
    transform_spec(TransformTarget::Gemini, UsageKind::None),
    // OpenAI models get
    transform_spec(TransformTarget::Gemini, UsageKind::None),

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
pub struct AistudioProvider {
    pool: CredentialPool<AistudioCredential>,
}

pub type AistudioCredential = BaseCredential;

impl AistudioProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<AistudioCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<AistudioCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for AistudioProvider {
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
impl DispatchProvider for AistudioProvider {
    fn dispatch_table(&self) -> &'static DispatchTable {
        &DISPATCH_TABLE
    }

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        match req {
            ProxyRequest::GeminiGenerate(request) => {
                self.handle_generate(request, false, ctx).await
            }
            ProxyRequest::GeminiGenerateStream(request) => {
                self.handle_generate_stream(request, ctx).await
            }
            ProxyRequest::GeminiCountTokens(request) => {
                self.handle_count_tokens(request, ctx).await
            }
            ProxyRequest::GeminiModelsList(request) => {
                self.handle_models_list(request, ctx).await
            }
            ProxyRequest::GeminiModelsGet(request) => {
                self.handle_models_get(request, ctx).await
            }
            ProxyRequest::OpenAIChat(request) => self.handle_openai_chat(request, false, ctx).await,
            ProxyRequest::OpenAIChatStream(request) => {
                self.handle_openai_chat(request, true, ctx).await
            }
            _ => Err(not_implemented(PROVIDER_NAME)),
        }
    }
}

impl AistudioProvider {
    async fn handle_generate(
        &self,
        request: gemini::generate_content::request::GenerateContentRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;
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
                    let path = format!("/v1beta/models/{model}:generateContent");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "gemini.generate",
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
                        operation: "gemini.generate".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response = handle_response(
                        response,
                        is_stream,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_generate_stream(
        &self,
        request: gemini::stream_content::request::StreamGenerateContentRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;
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
                    let path = format!("/v1beta/models/{model}:streamGenerateContent");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "gemini.stream_generate",
                        "POST",
                        &path,
                        Some(&model),
                        true,
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
                        operation: "gemini.stream_generate".to_string(),
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
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_count_tokens(
        &self,
        request: gemini::count_tokens::request::CountTokensRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;
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
                    let path = format!("/v1beta/models/{model}:countTokens");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "gemini.count_tokens",
                        "POST",
                        &path,
                        Some(&model),
                        false,
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
                        operation: "gemini.count_tokens".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response = handle_response(
                        response,
                        false,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_list(
        &self,
        request: gemini::list_models::request::ListModelsRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let query = request.query;
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let query = query.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let qs = serde_qs::to_string(&query).unwrap_or_default();
                    let mut path = "/v1beta/models".to_string();
                    if !qs.is_empty() {
                        path = format!("{path}?{qs}");
                    }
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "gemini.models_list",
                        "GET",
                        &path,
                        None,
                        false,
                        &scope,
                        || client.get(url).headers(req_headers.clone()).send(),
                    )
                    .await?;

                    let request_query = if qs.is_empty() { None } else { Some(qs) };
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "gemini.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: "/v1beta/models".to_string(),
                        request_query,
                        request_headers,
                        request_body: String::new(),
                    };
                    let response = handle_response(
                        response,
                        false,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_get(
        &self,
        request: gemini::get_model::request::GetModelRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let name = request.path.name;
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let name = name.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let path = format!("/v1beta/models/{name}");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "gemini.models_get",
                        "GET",
                        &path,
                        Some(&name),
                        false,
                        &scope,
                        || client.get(url).headers(req_headers.clone()).send(),
                    )
                    .await?;

                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "gemini.models_get".to_string(),
                        model: Some(name.clone()),
                        request_method: "GET".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body: String::new(),
                    };
                    let response = handle_response(
                        response,
                        false,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_openai_chat(
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
                    body.stream_options =
                        Some(openai::create_chat_completions::types::ChatCompletionStreamOptions {
                            include_usage: Some(true),
                            include_obfuscation: None,
                        });
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
                    let path = "/v1beta/openai/chat/completions".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_compat_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "openai.chat",
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
                        operation: "openai.chat".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response = handle_response(
                        response,
                        is_stream,
                        scope.clone(),
                        &ctx,
                        Some(meta.clone()),
                    )
                    .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }
}

#[allow(clippy::result_large_err)]
fn build_gemini_headers(api_key: &str) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-goog-api-key",
        HeaderValue::from_str(api_key).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(headers)
}

#[allow(clippy::result_large_err)]
fn build_openai_compat_headers(api_key: &str) -> Result<HeaderMap, AttemptFailure> {
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
    if let serde_json::Value::String(value) = &credential.secret {
        return Some(value.clone());
    }
    credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn build_url(base_url: Option<&str>, path: &str) -> String {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let mut path = path.trim_start_matches('/');
    if base.ends_with("/v1") && (path == "v1" || path.starts_with("v1/")) {
        path = path.trim_start_matches("v1/").trim_start_matches("v1");
    }
    if base.ends_with("/v1beta") && (path == "v1beta" || path.starts_with("v1beta/")) {
        path = path.trim_start_matches("v1beta/").trim_start_matches("v1beta");
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
