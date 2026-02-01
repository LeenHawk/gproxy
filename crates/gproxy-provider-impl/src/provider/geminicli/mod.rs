use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
use http::header::{AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use http::{HeaderMap, HeaderValue, StatusCode};
use uuid::Uuid;
use serde_json::{json, Value as JsonValue};

use gproxy_provider_core::{
    AttemptFailure, CredentialPool, DisallowScope, DownstreamContext, PoolSnapshot, Provider,
    ProxyRequest, ProxyResponse, StateSink, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta, StreamBody,
};
use gproxy_protocol::gemini;
use gproxy_protocol::sse::SseParser;

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

mod oauth;
mod refresh;

pub const PROVIDER_NAME: &str = "geminicli";
const DEFAULT_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
const GEMINICLI_USER_AGENT: &str = "GeminiCLI/0.1.5 (Windows; AMD64)";
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
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
    // OpenAI chat stream
    transform_spec(TransformTarget::Gemini, UsageKind::GeminiGenerate),
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
    // OAuth start
    native_spec(UsageKind::None),
    // OAuth callback
    native_spec(UsageKind::None),
    // Usage
    unsupported_spec(),
]);

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({ "base_url": DEFAULT_BASE_URL }),
        enabled: true,
    }
}

#[derive(Debug)]
pub struct GeminiCliProvider {
    pool: CredentialPool<GeminiCliCredential>,
}

pub type GeminiCliCredential = BaseCredential;

impl GeminiCliProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<GeminiCliCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<GeminiCliCredential>) {
        self.pool.replace_snapshot(snapshot);
    }
}

#[async_trait]
impl Provider for GeminiCliProvider {
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
impl DispatchProvider for GeminiCliProvider {
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
            ProxyRequest::OAuthStart { query, headers } => {
                oauth::handle_oauth_start(query, headers, ctx).await
            }
            ProxyRequest::OAuthCallback { query, headers } => {
                oauth::handle_oauth_callback(&self.pool, query, headers, ctx).await
            }
            _ => Err(UpstreamPassthroughError::service_unavailable(
                "non-native operation".to_string(),
            )),
        }
    }
}

impl GeminiCliProvider {
    async fn handle_generate(
        &self,
        request: gemini::generate_content::request::GenerateContentRequest,
        is_stream: bool,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = normalize_model_name(&request.path.model);
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
                    let tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let project_id = credential_project_id(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let path = "/v1internal:generateContent".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_headers(&tokens.access_token)?;
                    let user_prompt_id = Uuid::new_v4().to_string();
                    let wrapped = wrap_internal_request(&model, &project_id, &user_prompt_id, &body);
                    let request_body = json_body_to_string(&wrapped);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "geminicli.generate",
                        "POST",
                        &path,
                        Some(&model),
                        is_stream,
                        &scope,
                        || {
                            client
                                .post(url)
                                .headers(req_headers.clone())
                                .json(&wrapped)
                                .send()
                        },
                    )
                    .await?;
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "geminicli.generate".to_string(),
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
                    let response = unwrap_internal_json(response).map_err(|err| AttemptFailure {
                        passthrough: err,
                        mark: None,
                    })?;
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
        let model = normalize_model_name(&request.path.model);
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
                    let tokens = refresh::ensure_tokens(credential.value(), &ctx, &scope).await?;
                    let project_id = credential_project_id(credential.value())
                        .ok_or_else(|| invalid_credential(&scope, "missing project_id"))?;
                    let path = "/v1internal:streamGenerateContent?alt=sse".to_string();
                    let url = build_url(Some(&base_url), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_headers(&tokens.access_token)?;
                    let user_prompt_id = Uuid::new_v4().to_string();
                    let wrapped = wrap_internal_request(&model, &project_id, &user_prompt_id, &body);
                    let request_body = json_body_to_string(&wrapped);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "geminicli.stream",
                        "POST",
                        &path,
                        Some(&model),
                        true,
                        &scope,
                        || {
                            client
                                .post(url)
                                .headers(req_headers.clone())
                                .json(&wrapped)
                                .send()
                        },
                    )
                    .await?;
                    let meta = UpstreamRecordMeta {
                        provider: PROVIDER_NAME.to_string(),
                        provider_id: ctx.provider_id,
                        credential_id: Some(credential.value().id),
                        operation: "geminicli.stream".to_string(),
                        model: Some(model),
                        request_method: "POST".to_string(),
                        request_path: path,
                        request_query: None,
                        request_headers,
                        request_body,
                    };
                    let response = handle_response(response, true, scope.clone(), &ctx, Some(meta.clone()))
                        .await?;
                    let response = unwrap_internal_stream(response).map_err(|err| AttemptFailure {
                        passthrough: err,
                        mark: None,
                    })?;
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
        let model = normalize_model_name(&request.path.model);
        let _scope = DisallowScope::model(model.clone());
        let token_count = estimate_tokens(&request.body);
        let response_body = gemini::count_tokens::response::CountTokensResponse {
            total_tokens: token_count,
            cached_content_token_count: None,
            prompt_tokens_details: None,
            cache_tokens_details: None,
        };
        let body = serde_json::to_vec(&response_body)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let meta = UpstreamRecordMeta {
            provider: PROVIDER_NAME.to_string(),
            provider_id: ctx.provider_id,
            credential_id: None,
            operation: "geminicli.count_tokens".to_string(),
            model: Some(model),
            request_method: "POST".to_string(),
            request_path: "/v1beta/models:countTokens".to_string(),
            request_query: None,
            request_headers: String::new(),
            request_body: json_body_to_string(&request.body),
        };
        Ok(UpstreamOk {
            response: ProxyResponse::Json {
                status: StatusCode::OK,
                headers,
                body: Bytes::from(body),
            },
            meta,
        })
    }

    async fn handle_models_list(
        &self,
        request: gemini::list_models::request::ListModelsRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let response_body = load_models_list()?;
        let body = serde_json::to_vec(&response_body)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let meta = UpstreamRecordMeta {
            provider: PROVIDER_NAME.to_string(),
            provider_id: ctx.provider_id,
            credential_id: None,
            operation: "geminicli.models.list".to_string(),
            model: None,
            request_method: "GET".to_string(),
            request_path: "/v1beta/models".to_string(),
            request_query: request.query.page_token.clone(),
            request_headers: String::new(),
            request_body: String::new(),
        };
        Ok(UpstreamOk {
            response: ProxyResponse::Json {
                status: StatusCode::OK,
                headers,
                body: Bytes::from(body),
            },
            meta,
        })
    }

    async fn handle_models_get(
        &self,
        request: gemini::get_model::request::GetModelRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let name = normalize_model_name(&request.path.name);
        let response_body = load_models_list()?;
        let model = response_body
            .models
            .iter()
            .find(|item| normalize_model_name(&item.name) == name)
            .ok_or_else(|| {
                UpstreamPassthroughError::from_status(
                    StatusCode::NOT_FOUND,
                    format!("unknown model: {name}"),
                )
            })?;
        let body = serde_json::to_vec(&model)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let meta = UpstreamRecordMeta {
            provider: PROVIDER_NAME.to_string(),
            provider_id: ctx.provider_id,
            credential_id: None,
            operation: "geminicli.models.get".to_string(),
            model: Some(name.clone()),
            request_method: "GET".to_string(),
            request_path: format!("/v1beta/models/{name}"),
            request_query: None,
            request_headers: String::new(),
            request_body: String::new(),
        };
        Ok(UpstreamOk {
            response: ProxyResponse::Json {
                status: StatusCode::OK,
                headers,
                body: Bytes::from(body),
            },
            meta,
        })
    }
}

#[allow(clippy::result_large_err)]
fn build_headers(access_token: &str) -> Result<HeaderMap, AttemptFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access_token}")).map_err(|err| {
            AttemptFailure {
                passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
                mark: None,
            }
        })?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static(GEMINICLI_USER_AGENT));
    Ok(headers)
}

fn wrap_internal_request(
    model: &str,
    project_id: &str,
    user_prompt_id: &str,
    request: &gemini::generate_content::request::GenerateContentRequestBody,
) -> JsonValue {
    json!({
        "model": model,
        "project": project_id,
        "user_prompt_id": user_prompt_id,
        "request": request,
    })
}

#[allow(clippy::result_large_err)]
fn unwrap_internal_json(
    response: ProxyResponse,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match response {
        ProxyResponse::Json { status, headers, body } => {
            let parsed: JsonValue = serde_json::from_slice(&body)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            let unwrapped = unwrap_internal_value(parsed);
            let mapped = serde_json::to_vec(&unwrapped)
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            Ok(ProxyResponse::Json {
                status,
                headers,
                body: Bytes::from(mapped),
            })
        }
        ProxyResponse::Stream { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected json response".to_string(),
        )),
    }
}

#[allow(clippy::result_large_err)]
fn unwrap_internal_stream(
    response: ProxyResponse,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match response {
        ProxyResponse::Stream { status, headers, body } => {
            let stream = map_internal_stream(body.stream);
            Ok(ProxyResponse::Stream {
                status,
                headers,
                body: StreamBody::new("text/event-stream", stream),
            })
        }
        ProxyResponse::Json { .. } => Err(UpstreamPassthroughError::service_unavailable(
            "expected stream response".to_string(),
        )),
    }
}

fn map_internal_stream(
    upstream: impl futures_util::Stream<Item = Result<Bytes, io::Error>> + Unpin + Send + 'static,
) -> impl futures_util::Stream<Item = Result<Bytes, io::Error>> + Send {
    futures_util::stream::unfold(
        (upstream, SseParser::new(), VecDeque::<Bytes>::new()),
        move |(mut upstream, mut parser, mut pending)| async move {
            loop {
                if let Some(item) = pending.pop_front() {
                    return Some((Ok(item), (upstream, parser, pending)));
                }
                match upstream.next().await {
                    Some(Ok(bytes)) => {
                        for event in parser.push_bytes(&bytes) {
                            if event.data.is_empty() {
                                continue;
                            }
                            for mapped in map_event_data(&event.data) {
                                pending.push_back(mapped);
                            }
                        }
                        continue;
                    }
                    Some(Err(err)) => {
                        return Some((Err(err), (upstream, parser, pending)));
                    }
                    None => {
                        for event in parser.finish() {
                            if event.data.is_empty() {
                                continue;
                            }
                            for mapped in map_event_data(&event.data) {
                                pending.push_back(mapped);
                            }
                        }
                        if pending.is_empty() {
                            return None;
                        }
                    }
                }
            }
        },
    )
}

fn map_event_data(data: &str) -> Vec<Bytes> {
    if data == "[DONE]" {
        return vec![Bytes::from_static(b"data: [DONE]\n\n")];
    }
    let value: JsonValue = match serde_json::from_str(data) {
        Ok(value) => value,
        Err(_) => {
            let mut raw = Vec::with_capacity(data.len() + 8);
            raw.extend_from_slice(b"data: ");
            raw.extend_from_slice(data.as_bytes());
            raw.extend_from_slice(b"\n\n");
            return vec![Bytes::from(raw)];
        }
    };
    let mut out = Vec::new();
    match unwrap_internal_value(value) {
        JsonValue::Array(items) => {
            for item in items {
                if let Some(bytes) = sse_json_bytes(&unwrap_internal_value(item)) {
                    out.push(bytes);
                }
            }
        }
        other => {
            if let Some(bytes) = sse_json_bytes(&other) {
                out.push(bytes);
            }
        }
    }
    out
}

fn unwrap_internal_value(value: JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(mut map) => match map.remove("response") {
            Some(JsonValue::Object(mut inner)) => match inner.remove("response") {
                Some(nested) => nested,
                None => JsonValue::Object(inner),
            },
            Some(inner) => inner,
            None => JsonValue::Object(map),
        },
        other => other,
    }
}

fn sse_json_bytes<T: serde::Serialize>(value: &T) -> Option<Bytes> {
    let payload = serde_json::to_vec(value).ok()?;
    let mut data = Vec::with_capacity(payload.len() + 8);
    data.extend_from_slice(b"data: ");
    data.extend_from_slice(&payload);
    data.extend_from_slice(b"\n\n");
    Some(Bytes::from(data))
}

fn estimate_tokens(body: &gemini::count_tokens::request::CountTokensRequestBody) -> u32 {
    if let Some(contents) = body.contents.as_ref() {
        return estimate_tokens_from_contents(contents);
    }
    if let Some(req) = body.generate_content_request.as_ref() {
        if let Some(contents) = req.get("contents").and_then(|v| v.as_array()) {
            let mut text = String::new();
            for item in contents {
                if let Some(parts) = item.get("parts").and_then(|v| v.as_array()) {
                    for part in parts {
                        if let Some(value) = part.get("text").and_then(|v| v.as_str()) {
                            text.push_str(value);
                        }
                    }
                }
            }
            return estimate_tokens_from_text(&text);
        }
        let raw = serde_json::to_string(req).unwrap_or_default();
        return estimate_tokens_from_text(&raw);
    }
    0
}

fn estimate_tokens_from_contents(contents: &[gemini::count_tokens::types::Content]) -> u32 {
    let mut text = String::new();
    for content in contents {
        for part in &content.parts {
            if let Some(value) = part.text.as_ref() {
                text.push_str(value);
            }
        }
    }
    estimate_tokens_from_text(&text)
}

fn estimate_tokens_from_text(text: &str) -> u32 {
    let chars = text.chars().count() as u32;
    chars.div_ceil(4)
}

static MODELS_CACHE: OnceLock<gemini::list_models::response::ListModelsResponse> = OnceLock::new();

#[allow(clippy::result_large_err)]
fn load_models_list(
) -> Result<&'static gemini::list_models::response::ListModelsResponse, UpstreamPassthroughError> {
    if let Some(value) = MODELS_CACHE.get() {
        return Ok(value);
    }
    let raw = include_str!("models.json");
    let parsed: gemini::list_models::response::ListModelsResponse =
        serde_json::from_str(raw)
            .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    if parsed.models.is_empty() {
        return Err(UpstreamPassthroughError::service_unavailable(
            "models.json missing models".to_string(),
        ));
    }
    let _ = MODELS_CACHE.set(parsed);
    Ok(MODELS_CACHE
        .get()
        .expect("geminicli models cache initialized"))
}

fn normalize_model_name(model: &str) -> String {
    let mut name = model.trim();
    for prefix in [FAKE_PREFIX, ANTI_TRUNC_PREFIX] {
        if let Some(stripped) = name.strip_prefix(prefix) {
            name = stripped;
        }
    }
    if let Some(stripped) = name.strip_suffix(FAKE_SUFFIX) {
        name = stripped.trim_end_matches('-');
    }
    if let Some(stripped) = name.strip_suffix(ANTI_TRUNC_SUFFIX) {
        name = stripped.trim_end_matches('-');
    }
    name.to_string()
}

pub(super) fn credential_access_token(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("access_token")
        .or_else(|| credential.secret.get("token"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(super) fn credential_refresh_token(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn credential_project_id(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("project_id")
        .or_else(|| credential.meta.get("project_id"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(super) async fn channel_base_url(
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

const FAKE_PREFIX: &str = "\u{5047}\u{6d41}\u{5f0f}/";
const ANTI_TRUNC_PREFIX: &str = "\u{6d41}\u{5f0f}\u{6297}\u{622a}\u{65ad}/";
const FAKE_SUFFIX: &str = "\u{5047}\u{6d41}\u{5f0f}";
const ANTI_TRUNC_SUFFIX: &str = "\u{6d41}\u{5f0f}\u{6297}\u{622a}\u{65ad}";
