use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use http::header::{AUTHORIZATION, CONTENT_TYPE};
use http::{HeaderMap, HeaderValue};
use serde_json::json;
use tracing::{info, warn};

use gproxy_provider_core::{
    AttemptFailure, CallContext, CredentialPool, DisallowScope, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_protocol::{gemini, openai};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::{
    dispatch_request, CountTokensPlan, DispatchPlan, DispatchProvider, GenerateContentPlan,
    ModelsGetPlan, ModelsListPlan, StreamContentPlan, TransformPlan, UsageKind, UpstreamOk,
};
use crate::record::{headers_to_json, json_body_to_string};
use crate::upstream::{handle_response, network_failure};
use crate::ProviderDefault;
use crate::provider::not_implemented;

pub const PROVIDER_NAME: &str = "aistudio";
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({ "base_url": DEFAULT_BASE_URL }),
        enabled: true,
    }
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
        ctx: CallContext,
    ) -> Result<ProxyResponse, UpstreamPassthroughError> {
        dispatch_request(self, req, ctx).await
    }
}

#[async_trait]
impl DispatchProvider for AistudioProvider {
    fn dispatch_plan(&self, req: ProxyRequest) -> DispatchPlan {
        match req {
            ProxyRequest::GeminiGenerate { version, request } => DispatchPlan::Native {
                req: ProxyRequest::GeminiGenerate { version, request },
                usage: UsageKind::GeminiGenerate,
            },
            ProxyRequest::GeminiGenerateStream { version, request } => DispatchPlan::Native {
                req: ProxyRequest::GeminiGenerateStream { version, request },
                usage: UsageKind::GeminiGenerate,
            },
            ProxyRequest::GeminiCountTokens { version, request } => DispatchPlan::Native {
                req: ProxyRequest::GeminiCountTokens { version, request },
                usage: UsageKind::None,
            },
            ProxyRequest::GeminiModelsList { version, request } => DispatchPlan::Native {
                req: ProxyRequest::GeminiModelsList { version, request },
                usage: UsageKind::None,
            },
            ProxyRequest::GeminiModelsGet { version, request } => DispatchPlan::Native {
                req: ProxyRequest::GeminiModelsGet { version, request },
                usage: UsageKind::None,
            },
            ProxyRequest::OpenAIChat(request) => DispatchPlan::Native {
                req: ProxyRequest::OpenAIChat(request),
                usage: UsageKind::OpenAIChat,
            },
            ProxyRequest::OpenAIChatStream(request) => DispatchPlan::Native {
                req: ProxyRequest::OpenAIChatStream(request),
                usage: UsageKind::OpenAIChat,
            },
            ProxyRequest::OpenAIResponses(request) => DispatchPlan::Transform {
                plan: TransformPlan::GenerateContent(GenerateContentPlan::OpenAIResponses2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::OpenAIResponses,
            },
            ProxyRequest::OpenAIResponsesStream(request) => DispatchPlan::Transform {
                plan: TransformPlan::StreamContent(StreamContentPlan::OpenAIResponses2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::OpenAIResponses,
            },
            ProxyRequest::OpenAIInputTokens(request) => DispatchPlan::Transform {
                plan: TransformPlan::CountTokens(CountTokensPlan::OpenAIInputTokens2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
            ProxyRequest::OpenAIModelsList(request) => DispatchPlan::Transform {
                plan: TransformPlan::ModelsList(ModelsListPlan::OpenAI2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
            ProxyRequest::OpenAIModelsGet(request) => DispatchPlan::Transform {
                plan: TransformPlan::ModelsGet(ModelsGetPlan::OpenAI2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
            ProxyRequest::ClaudeMessages(request) => DispatchPlan::Transform {
                plan: TransformPlan::GenerateContent(GenerateContentPlan::Claude2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::ClaudeMessage,
            },
            ProxyRequest::ClaudeMessagesStream(request) => DispatchPlan::Transform {
                plan: TransformPlan::StreamContent(StreamContentPlan::Claude2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::ClaudeMessage,
            },
            ProxyRequest::ClaudeCountTokens(request) => DispatchPlan::Transform {
                plan: TransformPlan::CountTokens(CountTokensPlan::Claude2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
            ProxyRequest::ClaudeModelsList(request) => DispatchPlan::Transform {
                plan: TransformPlan::ModelsList(ModelsListPlan::Claude2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
            ProxyRequest::ClaudeModelsGet(request) => DispatchPlan::Transform {
                plan: TransformPlan::ModelsGet(ModelsGetPlan::Claude2Gemini {
                    version: gproxy_provider_core::GeminiApiVersion::V1Beta,
                    request,
                }),
                usage: UsageKind::None,
            },
        }
    }

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        match req {
            ProxyRequest::GeminiGenerate { version, request } => {
                self.handle_generate(version, request, false, ctx).await
            }
            ProxyRequest::GeminiGenerateStream { version, request } => {
                self.handle_generate_stream(version, request, ctx).await
            }
            ProxyRequest::GeminiCountTokens { version, request } => {
                self.handle_count_tokens(version, request, ctx).await
            }
            ProxyRequest::GeminiModelsList { version, request } => {
                self.handle_models_list(version, request, ctx).await
            }
            ProxyRequest::GeminiModelsGet { version, request } => {
                self.handle_models_get(version, request, ctx).await
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
        version: gproxy_provider_core::GeminiApiVersion,
        request: gemini::generate_content::request::GenerateContentRequest,
        is_stream: bool,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let version_prefix = version_prefix(version);
                    let path = format!("/{version_prefix}/models/{model}:generateContent");
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.generate",
                        method = "POST",
                        path = %path,
                        model = %model,
                        is_stream = is_stream
                    );
                    let response = client
                        .post(url)
                        .headers(req_headers.clone())
                        .json(&body)
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "gemini.generate",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.generate",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = is_stream
                    );
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
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
        version: gproxy_provider_core::GeminiApiVersion,
        request: gemini::stream_content::request::StreamGenerateContentRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let version_prefix = version_prefix(version);
                    let path = format!("/{version_prefix}/models/{model}:streamGenerateContent");
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.stream_generate",
                        method = "POST",
                        path = %path,
                        model = %model,
                        is_stream = true
                    );
                    let response = client
                        .post(url)
                        .headers(req_headers.clone())
                        .json(&body)
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "gemini.stream_generate",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.stream_generate",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = true
                    );
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
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
        version: gproxy_provider_core::GeminiApiVersion,
        request: gemini::count_tokens::request::CountTokensRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let version_prefix = version_prefix(version);
                    let path = format!("/{version_prefix}/models/{model}:countTokens");
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.count_tokens",
                        method = "POST",
                        path = %path,
                        model = %model,
                        is_stream = false
                    );
                    let response = client
                        .post(url)
                        .headers(req_headers.clone())
                        .json(&body)
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "gemini.count_tokens",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.count_tokens",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = false
                    );
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
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
        version: gproxy_provider_core::GeminiApiVersion,
        request: gemini::list_models::request::ListModelsRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let query = request.query;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let query = query.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let version_prefix = version_prefix(version);
                    let qs = serde_qs::to_string(&query).unwrap_or_default();
                    let mut path = format!("/{version_prefix}/models");
                    if !qs.is_empty() {
                        path = format!("{path}?{qs}");
                    }
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.models_list",
                        method = "GET",
                        path = %path,
                        is_stream = false
                    );
                    let response = client
                        .get(url)
                        .headers(req_headers.clone())
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "gemini.models_list",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.models_list",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = false
                    );

                    let request_query = if qs.is_empty() { None } else { Some(qs) };
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
                        credential_id: Some(credential.value().id),
                        operation: "gemini.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: format!("/{version_prefix}/models"),
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
        version: gproxy_provider_core::GeminiApiVersion,
        request: gemini::get_model::request::GetModelRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let name = request.path.name;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let name = name.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let version_prefix = version_prefix(version);
                    let path = format!("/{version_prefix}/models/{name}");
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_gemini_headers(&api_key)?;
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.models_get",
                        method = "GET",
                        path = %path,
                        is_stream = false
                    );
                    let response = client
                        .get(url)
                        .headers(req_headers.clone())
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "gemini.models_get",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "gemini.models_get",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = false
                    );

                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
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
        ctx: CallContext,
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

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                async move {
                    let api_key = credential_api_key(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing api_key"))?;
                    let base_url = credential_base_url(credential.value());
                    let path = "/v1beta/openai/chat/completions".to_string();
                    let url = build_url(base_url.as_deref(), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_openai_compat_headers(&api_key)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let started_at = Instant::now();
                    info!(
                        event = "upstream_request",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "openai.chat",
                        method = "POST",
                        path = %path,
                        model = %model,
                        is_stream = is_stream
                    );
                    let response = client
                        .post(url)
                        .headers(req_headers.clone())
                        .json(&body)
                        .send()
                        .await
                        .map_err(|err| {
                            warn!(
                                event = "upstream_response",
                                trace_id = %ctx.trace_id,
                                provider = %PROVIDER_NAME,
                                op = "openai.chat",
                                status = "error",
                                elapsed_ms = started_at.elapsed().as_millis(),
                                error = %err
                            );
                            network_failure(err, &scope)
                        })?;
                    info!(
                        event = "upstream_response",
                        trace_id = %ctx.trace_id,
                        provider = %PROVIDER_NAME,
                        op = "openai.chat",
                        status = %response.status().as_u16(),
                        elapsed_ms = started_at.elapsed().as_millis(),
                        is_stream = is_stream
                    );
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx
                            .downstream_meta
                            .as_ref()
                            .and_then(|meta| meta.provider_id),
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

fn credential_base_url(credential: &BaseCredential) -> Option<String> {
    credential
        .meta
        .get("base_url")
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

fn version_prefix(version: gproxy_provider_core::GeminiApiVersion) -> &'static str {
    match version {
        gproxy_provider_core::GeminiApiVersion::V1 => "v1",
        gproxy_provider_core::GeminiApiVersion::V1Beta => "v1beta",
    }
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
