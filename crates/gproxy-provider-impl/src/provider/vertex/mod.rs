use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use bytes::Bytes;
use http::header::{AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, TRANSFER_ENCODING};
use http::{HeaderMap, HeaderValue};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::json;

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, DownstreamContext, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta,
};
use gproxy_protocol::{gemini, openai};

use crate::credential::BaseCredential;
use crate::ProviderDefault;
use crate::client::shared_client;
use crate::dispatch::{
    dispatch_request, DispatchProvider, DispatchTable, TransformTarget, UsageKind, UpstreamOk,
    native_spec, transform_spec, unsupported_spec,
};
use crate::record::{headers_to_json, json_body_to_string};
use crate::storage::global_storage;
use crate::upstream::{handle_response, send_with_logging};

pub const PROVIDER_NAME: &str = "vertex";
const DEFAULT_BASE_URL: &str = "https://aiplatform.googleapis.com";
const DEFAULT_LOCATION: &str = "us-central1";
const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
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
        config_json: json!({
            "base_url": DEFAULT_BASE_URL,
            "location": DEFAULT_LOCATION,
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": DEFAULT_TOKEN_URI,
            "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
            "client_x509_cert_url": "https://www.googleapis.com/robot/v1/metadata/x509/REPLACE_ME",
            "universe_domain": "googleapis.com",
            "oauth_auth_url": "https://accounts.google.com/o/oauth2/auth",
            "oauth_token_url": DEFAULT_TOKEN_URI,
        }),
        enabled: true,
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct ServiceAccountKey {
    #[serde(rename = "type")]
    r#type: String,
    project_id: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    client_id: String,
}

#[derive(Debug)]
pub struct VertexProvider {
    pool: CredentialPool<VertexCredential>,
}

pub type VertexCredential = BaseCredential;

impl VertexProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<VertexCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<VertexCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for VertexProvider {
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
impl DispatchProvider for VertexProvider {
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
            ProxyRequest::OpenAIChat(request) => {
                self.handle_openai_chat(request, false, ctx).await
            }
            ProxyRequest::OpenAIChatStream(request) => {
                self.handle_openai_chat(request, true, ctx).await
            }
            _ => Err(UpstreamPassthroughError::service_unavailable(
                "non-native operation".to_string(),
            )),
        }
    }
}

impl VertexProvider {
    async fn handle_generate(
        &self,
        request: gemini::generate_content::request::GenerateContentRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model.clone();
        let scope = DisallowScope::model(model.clone());
        let body = request.body;
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let model_id = normalize_model_name(&model);
                    let path = format!(
                        "/v1beta1/projects/{project_id}/locations/{location}/publishers/google/models/{model_id}:generateContent"
                    );
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
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
                    let response =
                        handle_response(response, is_stream, scope.clone(), &ctx, Some(meta.clone()))
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
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let model_id = normalize_model_name(&model);
                    let path = format!(
                        "/v1beta1/projects/{project_id}/locations/{location}/publishers/google/models/{model_id}:streamGenerateContent"
                    );
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
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
                    let response =
                        handle_response(response, true, scope.clone(), &ctx, Some(meta.clone()))
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
        let mut body = request.body;
        if body.contents.is_none()
            && let Some(generate_request) = body.generate_content_request.as_ref()
                && let Some(contents) = generate_request.get("contents")
                    && let Ok(contents) = serde_json::from_value(contents.clone()) {
                        body.contents = Some(contents);
                        body.generate_content_request = None;
                    }
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let model_id = normalize_model_name(&model);
                    let path = format!(
                        "/v1beta1/projects/{project_id}/locations/{location}/publishers/google/models/{model_id}:countTokens"
                    );
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
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
                    let response =
                        handle_response(response, false, scope.clone(), &ctx, Some(meta.clone()))
                            .await?;
                    Ok(UpstreamOk { response, meta })
                }
            })
            .await
    }

    async fn handle_models_list(
        &self,
        _request: gemini::list_models::request::ListModelsRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let scope = DisallowScope::AllModels;
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let _project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let path = "/v1beta1/publishers/google/models".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
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
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "gemini.models_list".to_string(),
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
                    let response = map_vertex_models_list_response(response)?;
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
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let name = name.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let _project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let model_id = normalize_model_name(&name);
                    let path = format!("/v1beta1/publishers/google/models/{model_id}");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
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
                        model: Some(name),
                        request_method: "GET".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body: String::new(),
                    };
                    let response =
                        handle_response(response, false, scope.clone(), &ctx, Some(meta.clone()))
                            .await?;
                    let response = map_vertex_model_get_response(response)?;
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
        body.model = normalize_vertex_openai_model(&body.model);
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
        let channel = channel_config(&ctx).await?;

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let scope = scope.clone();
                let model = model.clone();
                let body = body.clone();
                let channel = channel.clone();
                async move {
                    let sa = credential_service_account(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing service_account"))?;
                    let project_id = credential_project_id(credential.value(), &sa)
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let location = channel.location.clone();
                    let base_url = resolve_vertex_base_url(Some(&channel.base_url), &location);
                    let endpoint_path =
                        format!("projects/{project_id}/locations/{location}/endpoints/openapi");
                    let path = format!("/v1beta1/{endpoint_path}/chat/completions");
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let access_token = fetch_access_token(&client, &sa, &channel.token_uri).await?;
                    let req_headers = build_vertex_headers(&access_token)?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "openai.chat.completions",
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
                        operation: "openai.chat.completions".to_string(),
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
}

#[allow(clippy::result_large_err)]
fn build_vertex_headers(access_token: &str) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    let mut bearer = String::with_capacity(access_token.len() + 7);
    bearer.push_str("Bearer ");
    bearer.push_str(access_token);
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

fn credential_service_account(credential: &BaseCredential) -> Option<ServiceAccountKey> {
    if let serde_json::Value::String(value) = &credential.secret {
        return serde_json::from_str(value).ok();
    }
    if let Some(secret) = credential.secret.get("service_account_json") {
        if secret.is_string() {
            return secret
                .as_str()
                .and_then(|value| serde_json::from_str(value).ok());
        }
        return serde_json::from_value(secret.clone()).ok();
    }
    serde_json::from_value(credential.secret.clone()).ok()
}

fn credential_project_id(credential: &BaseCredential, sa: &ServiceAccountKey) -> Option<String> {
    credential
        .meta
        .get("project_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| Some(sa.project_id.clone()))
}

#[derive(Debug, Clone)]
struct VertexChannelConfig {
    base_url: String,
    location: String,
    token_uri: String,
}

async fn channel_config(
    ctx: &UpstreamContext,
) -> Result<VertexChannelConfig, UpstreamPassthroughError> {
    let mut base_url = DEFAULT_BASE_URL.to_string();
    let mut location = DEFAULT_LOCATION.to_string();
    let mut token_uri = DEFAULT_TOKEN_URI.to_string();
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
            && let Some(map) = provider.config_json.as_object() {
                if let Some(value) = map.get("base_url").and_then(|v| v.as_str()) {
                    base_url = value.to_string();
                }
                if let Some(value) = map.get("location").and_then(|v| v.as_str()) {
                    location = value.to_string();
                }
                if let Some(value) = map.get("token_uri").and_then(|v| v.as_str()) {
                    token_uri = value.to_string();
                }
            }
    }
    Ok(VertexChannelConfig {
        base_url: base_url.trim_end_matches('/').to_string(),
        location,
        token_uri,
    })
}

fn resolve_vertex_base_url(base_url: Option<&str>, location: &str) -> String {
    let base_url = base_url.unwrap_or(DEFAULT_BASE_URL);
    if base_url == DEFAULT_BASE_URL && location != "global" {
        format!("https://{location}-aiplatform.googleapis.com")
    } else {
        base_url.to_string()
    }
}


fn normalize_model_name(name: &str) -> String {
    let name = name.strip_prefix("models/").unwrap_or(name);
    let name = name
        .strip_prefix("publishers/google/models/")
        .unwrap_or(name);
    name.to_string()
}

fn normalize_vertex_openai_model(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    if let Some(stripped) = trimmed.strip_prefix("publishers/")
        && let Some((publisher, model_name)) = stripped.split_once("/models/")
    {
        return format!("{publisher}/{model_name}");
    }
    if let Some(idx) = trimmed.find("/publishers/") {
        let tail = &trimmed[(idx + "/publishers/".len())..];
        if let Some((publisher, model_name)) = tail.split_once("/models/") {
            return format!("{publisher}/{model_name}");
        }
    }
    if let Some(stripped) = trimmed.strip_prefix("models/") {
        return format!("google/{stripped}");
    }
    if trimmed.contains('/') {
        return trimmed.to_string();
    }
    format!("google/{trimmed}")
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModel {
    name: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, alias = "versionId")]
    version: Option<String>,
    #[serde(default)]
    input_token_limit: Option<i64>,
    #[serde(default)]
    output_token_limit: Option<i64>,
    #[serde(default)]
    supported_generation_methods: Option<Vec<String>>,
    #[serde(default)]
    thinking: Option<bool>,
    #[serde(default)]
    temperature: Option<f64>,
    #[serde(default)]
    max_temperature: Option<f64>,
    #[serde(default)]
    top_p: Option<f64>,
    #[serde(default)]
    top_k: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexPublisherModelsListResponse {
    #[serde(default)]
    publisher_models: Vec<VertexPublisherModel>,
    #[serde(default)]
    next_page_token: Option<String>,
}

fn map_vertex_model(model: VertexPublisherModel) -> gemini::get_model::types::Model {
    let id = normalize_model_name(&model.name);
    let version = model.version.unwrap_or_else(|| "unknown".to_string());
    gemini::get_model::types::Model {
        name: format!("models/{id}"),
        base_model_id: None,
        version,
        display_name: model.display_name,
        description: model.description,
        input_token_limit: model
            .input_token_limit
            .and_then(|value| u32::try_from(value).ok()),
        output_token_limit: model
            .output_token_limit
            .and_then(|value| u32::try_from(value).ok()),
        supported_generation_methods: model.supported_generation_methods,
        thinking: model.thinking,
        temperature: model.temperature,
        max_temperature: model.max_temperature,
        top_p: model.top_p,
        top_k: model.top_k.and_then(|value| u32::try_from(value).ok()),
    }
}

#[allow(clippy::result_large_err)]
fn map_vertex_models_list_response(
    response: ProxyResponse,
) -> Result<ProxyResponse, AttemptFailure> {
    match response {
        ProxyResponse::Json {
            status,
            mut headers,
            body,
        } => {
            let parsed = serde_json::from_slice::<VertexPublisherModelsListResponse>(&body)
                .map_err(|err| AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                })?;
            let mapped = gemini::list_models::response::ListModelsResponse {
                models: parsed
                    .publisher_models
                    .into_iter()
                    .map(map_vertex_model)
                    .collect(),
                next_page_token: parsed.next_page_token,
            };
            let mapped_body = serde_json::to_vec(&mapped)
                .map_err(|err| AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                })?;
            scrub_headers(&mut headers);
            Ok(ProxyResponse::Json {
                status,
                headers,
                body: Bytes::from(mapped_body),
            })
        }
        ProxyResponse::Stream { .. } => Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(
                "expected json response".to_string(),
            ),
            mark: None,
        }),
    }
}

#[allow(clippy::result_large_err)]
fn map_vertex_model_get_response(
    response: ProxyResponse,
) -> Result<ProxyResponse, AttemptFailure> {
    match response {
        ProxyResponse::Json {
            status,
            mut headers,
            body,
        } => {
            let parsed = serde_json::from_slice::<VertexPublisherModel>(&body)
                .map_err(|err| AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                })?;
            let mapped = map_vertex_model(parsed);
            let mapped_body = serde_json::to_vec(&mapped)
                .map_err(|err| AttemptFailure {
                    passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                    mark: None,
                })?;
            scrub_headers(&mut headers);
            Ok(ProxyResponse::Json {
                status,
                headers,
                body: Bytes::from(mapped_body),
            })
        }
        ProxyResponse::Stream { .. } => Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(
                "expected json response".to_string(),
            ),
            mark: None,
        }),
    }
}

fn scrub_headers(headers: &mut HeaderMap) {
    headers.remove(CONTENT_LENGTH);
    headers.remove(TRANSFER_ENCODING);
}

async fn fetch_access_token(
    client: &Arc<wreq::Client>,
    sa: &ServiceAccountKey,
    token_uri: &str,
) -> Result<String, AttemptFailure> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?
        .as_secs() as i64;
    let claims = JwtClaims {
        iss: sa.client_email.clone(),
        sub: sa.client_email.clone(),
        aud: token_uri.to_string(),
        scope: DEFAULT_SCOPE.to_string(),
        iat: now,
        exp: now + 3600,
    };
    let header = Header {
        alg: Algorithm::RS256,
        ..Header::default()
    };
    let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes()).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    let jwt = jsonwebtoken::encode(&header, &claims, &key).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;

    let response = client
        .post(token_uri)
        .form(&[
            (
                "grant_type",
                "urn:ietf:params:oauth:grant-type:jwt-bearer",
            ),
            ("assertion", jwt.as_str()),
        ])
        .send()
        .await
        .map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    if !status.is_success() {
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(format!(
                "token exchange failed: {}",
                String::from_utf8_lossy(&body)
            )),
            mark: None,
        });
    }
    let token: TokenResponse = serde_json::from_slice(&body).map_err(|err| AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: None,
    })?;
    Ok(token.access_token)
}

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    aud: String,
    scope: String,
    iat: i64,
    exp: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

fn build_url(base_url: Option<&str>, path: &str) -> String {
    let base = base_url.unwrap_or(DEFAULT_BASE_URL).trim_end_matches('/');
    let path = path.trim_start_matches('/');
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
