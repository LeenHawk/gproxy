use std::sync::Arc;
use async_trait::async_trait;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue};
use serde_json::{json, Value as JsonValue};

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowLevel, DisallowMark, DisallowScope, DownstreamContext,
    PoolSnapshot, Provider, ProxyRequest, ProxyResponse, StateSink, UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_protocol::claude;
use gproxy_protocol::openai;
use gproxy_protocol::claude::types::{AnthropicBetaHeader, AnthropicVersion};
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

pub const PROVIDER_NAME: &str = "claude";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DISPATCH_TABLE: DispatchTable = DispatchTable::new([
    // Claude messages
    native_spec(UsageKind::ClaudeMessage),
    // Claude messages stream
    native_spec(UsageKind::ClaudeMessage),
    // Claude count tokens
    native_spec(UsageKind::None),
    // Claude models list
    native_spec(UsageKind::None),
    // Claude models get
    native_spec(UsageKind::None),
    // Gemini generate
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
    // Gemini generate stream
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
    // Gemini count tokens
    transform_spec(TransformTarget::Claude, UsageKind::None),
    // Gemini models list
    transform_spec(TransformTarget::Claude, UsageKind::None),
    // Gemini models get
    transform_spec(TransformTarget::Claude, UsageKind::None),
    // OpenAI chat
    native_spec(UsageKind::OpenAIChat),
    // OpenAI chat stream
    native_spec(UsageKind::OpenAIChat),
    // OpenAI responses
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
    // OpenAI responses stream
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
    // OpenAI input tokens
    transform_spec(TransformTarget::Claude, UsageKind::None),
    // OpenAI models list
    transform_spec(TransformTarget::Claude, UsageKind::None),
    // OpenAI models get
    transform_spec(TransformTarget::Claude, UsageKind::None),

    // Codex oauth start
    unsupported_spec(),
    // Codex oauth callback
    unsupported_spec(),
    // Codex usage
    unsupported_spec(),
]);
const HEADER_API_KEY: &str = "x-api-key";
const HEADER_VERSION: &str = "anthropic-version";
const HEADER_BETA: &str = "anthropic-beta";

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
pub struct ClaudeProvider {
    pool: CredentialPool<ClaudeCredential>,
}

pub type ClaudeCredential = BaseCredential;

impl ClaudeProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<ClaudeCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<ClaudeCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for ClaudeProvider {
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
impl DispatchProvider for ClaudeProvider {
    fn dispatch_table(&self) -> &'static DispatchTable {
        &DISPATCH_TABLE
    }

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        match req {
            ProxyRequest::ClaudeMessages(request) => self.handle_messages(request, false, ctx).await,
            ProxyRequest::ClaudeMessagesStream(request) => {
                self.handle_messages(request, true, ctx).await
            }
            ProxyRequest::ClaudeCountTokens(request) => self.handle_count_tokens(request, ctx).await,
            ProxyRequest::ClaudeModelsList(request) => self.handle_models_list(request, ctx).await,
            ProxyRequest::ClaudeModelsGet(request) => self.handle_models_get(request, ctx).await,
            ProxyRequest::OpenAIChat(request) => {
                self.handle_openai_chat_passthrough(request, false, ctx).await
            }
            ProxyRequest::OpenAIChatStream(request) => {
                self.handle_openai_chat_passthrough(request, true, ctx).await
            }
            _ => Err(UpstreamPassthroughError::service_unavailable(
                "non-native operation".to_string(),
            )),
        }
    }
}

impl ClaudeProvider {
    async fn handle_messages(
        &self,
        request: claude::create_message::request::CreateMessageRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = model_to_string(&request.body.model);
        let scope = DisallowScope::model(model.clone());
        let headers = request.headers;
        let mut body = request.body;
        if is_stream {
            body.stream = Some(true);
        }
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let body = body.clone();
                let scope = scope.clone();
                let model = model.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let url = build_url(Some(&base_url), "/v1/messages");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers =
                        build_headers(&api_key, headers.anthropic_version, headers.anthropic_beta)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claude.messages",
                        "POST",
                        "/v1/messages",
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
                        operation: "claude.messages".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: "/v1/messages".to_string(),
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
                    // Pass-through response for now; per-op fixups can be added inline if needed.
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_count_tokens(
        &self,
        request: claude::count_tokens::request::CountTokensRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = model_to_string(&request.body.model);
        let scope = DisallowScope::model(model.clone());
        let headers = request.headers;
        let body = request.body;
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let body = body.clone();
                let scope = scope.clone();
                let model = model.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let url = build_url(Some(&base_url), "/v1/messages/count_tokens");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers =
                        build_headers(&api_key, headers.anthropic_version, headers.anthropic_beta)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claude.count_tokens",
                        "POST",
                        "/v1/messages/count_tokens",
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
                        operation: "claude.count_tokens".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: "/v1/messages/count_tokens".to_string(),
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
                    // Pass-through response for now; per-op fixups can be added inline if needed.
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_list(
        &self,
        request: claude::list_models::request::ListModelsRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let headers = request.headers;
        let query = request.query;
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let query = query.clone();
                let scope = scope.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let qs = serde_qs::to_string(&query).unwrap_or_default();
                    let mut url = build_url(Some(&base_url), "/v1/models");
                    if !qs.is_empty() {
                        url = format!("{url}?{qs}");
                    }
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers =
                        build_headers(&api_key, headers.anthropic_version, headers.anthropic_beta)?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claude.models_list",
                        "GET",
                        "/v1/models",
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
                        operation: "claude.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: "/v1/models".to_string(),
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
                    // Pass-through response for now; per-op fixups can be added inline if needed.
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_get(
        &self,
        request: claude::get_model::request::GetModelRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::model(request.path.model_id.clone());
        let headers = request.headers;
        let model_id = request.path.model_id;
        let base_url = channel_base_url(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let model_id = model_id.clone();
                let scope = scope.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let url = build_url(
                        Some(&base_url),
                        &format!("/v1/models/{model_id}"),
                    );
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers =
                        build_headers(&api_key, headers.anthropic_version, headers.anthropic_beta)?;
                    let request_headers = headers_to_json(&req_headers);
                    let path = format!("/v1/models/{model_id}");
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claude.models_get",
                        "GET",
                        &path,
                        Some(&model_id),
                        false,
                        &scope,
                        || client.get(url).headers(req_headers.clone()).send(),
                    )
                    .await?;

                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "claude.models_get".to_string(),
                        model: Some(model_id.clone()),
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
                    // Pass-through response for now; per-op fixups can be added inline if needed.
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_openai_chat_passthrough(
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
            .execute(scope.clone(), move |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let base_url = base_url.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let url = build_url(Some(&base_url), "/v1/chat/completions");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_compat_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "openai.chat.completions",
                        "POST",
                        "/v1/chat/completions",
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
                        operation: "openai.chat.completions".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: "/v1/chat/completions".to_string(),
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
                    // Pass-through response for now; per-op fixups can be added inline if needed.
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }
}

#[allow(clippy::result_large_err)]
fn build_headers(
    api_key: &str,
    version: AnthropicVersion,
    beta: Option<AnthropicBetaHeader>,
) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        HEADER_API_KEY,
        HeaderValue::from_str(api_key).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    let version_value = match version {
        AnthropicVersion::V20230601 => "2023-06-01",
        AnthropicVersion::V20230101 => "2023-01-01",
    };
    headers.insert(HEADER_VERSION, HeaderValue::from_static(version_value));
    if let Some(beta_header) = beta {
        let values = match beta_header {
            AnthropicBetaHeader::Single(beta) => vec![match serde_json::to_value(beta) {
                Ok(JsonValue::String(value)) => value,
                _ => "unknown".to_string(),
            }],
            AnthropicBetaHeader::Multiple(list) => list
                .into_iter()
                .map(|beta| match serde_json::to_value(beta) {
                    Ok(JsonValue::String(value)) => value,
                    _ => "unknown".to_string(),
                })
                .collect(),
        };
        if !values.is_empty() {
            let beta_value = values.join(",");
            headers.insert(
                HEADER_BETA,
                HeaderValue::from_str(&beta_value).map_err(|err| AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                })?,
            );
        }
    }
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
    if let JsonValue::String(value) = &credential.secret {
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
    format!("{base}/{path}")
}

fn model_to_string(model: &claude::count_tokens::types::Model) -> String {
    match serde_json::to_value(model) {
        Ok(JsonValue::String(value)) => value,
        _ => "unknown".to_string(),
    }
}

fn invalid_credential(scope: &DisallowScope, message: &str) -> AttemptFailure {
    AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(message.to_string()),
        mark: Some(DisallowMark {
            scope: scope.clone(),
            level: DisallowLevel::Dead,
            duration: None,
            reason: Some(message.to_string()),
        }),
    }
}
