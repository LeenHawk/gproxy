use std::sync::Arc;

use async_trait::async_trait;
use http::header::{AUTHORIZATION, CONTENT_TYPE, USER_AGENT};
use http::{HeaderMap, HeaderValue};
use serde_json::{json, Value as JsonValue};

use gproxy_provider_core::{
    AttemptFailure, CredentialEntry, CredentialPool, DisallowLevel, DisallowMark, DisallowScope,
    DownstreamContext, PoolSnapshot, Provider, ProxyRequest, ProxyResponse, StateSink,
    UpstreamContext,
    UpstreamPassthroughError, UpstreamRecordMeta,
};
use gproxy_protocol::claude;
use gproxy_protocol::claude::count_tokens::types::{
    BetaSystemParam, BetaTextBlockParam, BetaTextBlockType,
};
use gproxy_protocol::claude::types::{AnthropicBetaHeader, AnthropicVersion};

use crate::client::shared_client;
use crate::credential::BaseCredential;
use crate::dispatch::{
    dispatch_request, DispatchProvider, DispatchTable, TransformTarget, UsageKind, UpstreamOk,
    native_spec, transform_spec,
};
use crate::record::{headers_to_json, json_body_to_string};
use crate::upstream::{handle_response, send_with_logging};
use crate::ProviderDefault;
use crate::storage::global_storage;

use gproxy_storage::AdminCredentialInput;

mod oauth;
mod refresh;
mod usage;

pub const PROVIDER_NAME: &str = "claudecode";
const DEFAULT_API_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_CLAUDE_AI_BASE_URL: &str = "https://claude.ai";
const DEFAULT_CONSOLE_BASE_URL: &str = "https://console.anthropic.com";
const HEADER_VERSION: &str = "anthropic-version";
const HEADER_BETA: &str = "anthropic-beta";
const CLAUDE_CODE_UA: &str = "claude-code/2.1.27";
const CLAUDE_CODE_SYSTEM_PRELUDE: &str =
    "You are a Claude agent, built on Anthropic's Claude Agent SDK.";
const CLAUDE_BETA_CONTEXT_1M: &str = "context-1m-2025-08-07";
pub(super) const TOKEN_UA: &str = "claude-cli/2.1.27 (external, cli)";
pub(super) const COOKIE_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
pub(super) const OAUTH_BETA: &str = "oauth-2025-04-20";
pub(super) const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub(super) const OAUTH_SCOPE: &str =
    "org:create_api_key user:profile user:inference user:sessions:claude_code";
pub(super) const OAUTH_SCOPE_SESSION: &str =
    "user:profile user:inference user:sessions:claude_code";
pub(super) const OAUTH_SCOPE_SETUP: &str = "user:inference";

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
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
    // OpenAI chat stream
    transform_spec(TransformTarget::Claude, UsageKind::ClaudeMessage),
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
    // OAuth start
    native_spec(UsageKind::None),
    // OAuth callback
    native_spec(UsageKind::None),
    // Usage
    native_spec(UsageKind::None),
]);

pub fn default_provider() -> ProviderDefault {
    ProviderDefault {
        name: PROVIDER_NAME,
        config_json: json!({
            "base_url": DEFAULT_API_BASE_URL,
            "claude_ai_base_url": DEFAULT_CLAUDE_AI_BASE_URL,
            "console_base_url": DEFAULT_CONSOLE_BASE_URL,
        }),
        enabled: true,
    }
}

#[derive(Debug)]
pub struct ClaudeCodeProvider {
    pool: CredentialPool<ClaudeCodeCredential>,
}

pub type ClaudeCodeCredential = BaseCredential;

impl ClaudeCodeProvider {
    pub fn new(sink: Arc<dyn StateSink>) -> Self {
        let snapshot = PoolSnapshot::empty();
        let pool = CredentialPool::new(PROVIDER_NAME, snapshot, Some(sink));
        Self { pool }
    }

    pub fn pool(&self) -> &CredentialPool<ClaudeCodeCredential> {
        &self.pool
    }

    pub fn replace_snapshot(&self, snapshot: PoolSnapshot<ClaudeCodeCredential>) {
        self.pool.replace_snapshot(snapshot);
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
impl Provider for ClaudeCodeProvider {
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
impl DispatchProvider for ClaudeCodeProvider {
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

impl ClaudeCodeProvider {
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
    apply_claude_code_system(&mut body.system, ctx.user_agent.as_deref());

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let body = body.clone();
                let scope = scope.clone();
                let model = model.clone();
                let pool = &self.pool;
                async move {
                    let tokens = refresh::ensure_tokens(pool, credential.value(), &ctx, &scope).await?;
                    let access_token = tokens.access_token.clone();
                    let channel = channel_urls(&ctx)
                        .await
                        .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;
                    let url = build_url(Some(channel.api_base.as_str()), "/v1/messages");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let supports_1m = credential_supports_1m(credential.value());
                    let is_sonnet = is_sonnet4_model(&model);
                    let attempts: Vec<bool> = if is_sonnet {
                        match supports_1m {
                            Some(true) => vec![true],
                            Some(false) => vec![false],
                            None => vec![true, false],
                        }
                    } else {
                        vec![false]
                    };

                    let version = headers.anthropic_version;
                    let beta = headers.anthropic_beta.clone();
                    for (idx, use_1m) in attempts.iter().copied().enumerate() {
                        let mut req_headers = build_headers(
                            &access_token,
                            version,
                            beta.clone(),
                            ctx.user_agent.as_deref(),
                        )?;
                        if use_1m {
                            add_beta_value(&mut req_headers, CLAUDE_BETA_CONTEXT_1M)?;
                        }
                        let request_body = json_body_to_string(&body);
                        let request_headers = headers_to_json(&req_headers);
                        let response = send_with_logging(
                            &ctx,
                            PROVIDER_NAME,
                            "claudecode.messages",
                            "POST",
                            "/v1/messages",
                            Some(&model),
                            is_stream,
                            &scope,
                            || {
                                client
                                    .post(&url)
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
                            operation: "claudecode.messages".to_string(),
                            model: Some(model.clone()),
                            request_method: "POST".to_string(),
                            request_path: "/v1/messages".to_string(),
                            request_query: None,
                            request_headers,
                            request_body,
                        };
                        match handle_response(
                            response,
                            is_stream,
                            scope.clone(),
                            &ctx,
                            Some(meta.clone()),
                        )
                        .await {
                            Ok(response) => {
                                if is_sonnet && use_1m && supports_1m != Some(true) {
                                    let _ = persist_claude_1m_support(
                                        pool,
                                        &ctx,
                                        credential.value(),
                                        true,
                                    )
                                    .await;
                                }
                                return Ok(UpstreamOk { response, meta });
                            }
                            Err(err) => {
                                let is_last = idx + 1 == attempts.len();
                                let can_fallback =
                                    use_1m && is_sonnet && !is_last && is_context_1m_forbidden(&err.passthrough);
                                if can_fallback {
                                    if supports_1m != Some(false) {
                                        let _ = persist_claude_1m_support(
                                            pool,
                                            &ctx,
                                            credential.value(),
                                            false,
                                        )
                                        .await;
                                    }
                                    continue;
                                }
                                return Err(err);
                            }
                        }
                    }

                    Err(invalid_credential(&scope, "claude messages failed"))
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
    let mut body = request.body;
    apply_claude_code_system(&mut body.system, ctx.user_agent.as_deref());

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let body = body.clone();
                let scope = scope.clone();
                let model = model.clone();
                let pool = &self.pool;
                async move {
                    let tokens = refresh::ensure_tokens(pool, credential.value(), &ctx, &scope).await?;
                    let access_token = tokens.access_token.clone();
                    let channel = channel_urls(&ctx)
                        .await
                        .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;
                    let url = build_url(Some(channel.api_base.as_str()), "/v1/messages/count_tokens");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_headers(
                        &access_token,
                        headers.anthropic_version,
                        headers.anthropic_beta,
                        ctx.user_agent.as_deref(),
                    )?;
                    let request_body = json_body_to_string(&body);
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claudecode.count_tokens",
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
                        operation: "claudecode.count_tokens".to_string(),
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

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let scope = scope.clone();
                let pool = &self.pool;
                async move {
                    let tokens = refresh::ensure_tokens(pool, credential.value(), &ctx, &scope).await?;
                    let access_token = tokens.access_token.clone();
                    let channel = channel_urls(&ctx)
                        .await
                        .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;
                    let url = build_url(Some(channel.api_base.as_str()), "/v1/models");
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_headers(
                        &access_token,
                        headers.anthropic_version,
                        headers.anthropic_beta,
                        ctx.user_agent.as_deref(),
                    )?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claudecode.models_list",
                        "GET",
                        "/v1/models",
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
                        operation: "claudecode.models_list".to_string(),
                        model: None,
                        request_method: "GET".to_string(),
                        request_path: "/v1/models".to_string(),
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

    async fn handle_models_get(
        &self,
        request: claude::get_model::request::GetModelRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError> {
        let model = request.path.model_id.clone();
        let scope = DisallowScope::model(model.clone());
        let headers = request.headers;
        let path = format!("/v1/models/{model}");

        self.pool
            .execute(scope.clone(), |credential| {
                let ctx = ctx.clone();
                let headers = headers.clone();
                let scope = scope.clone();
                let model = model.clone();
                let path = path.clone();
                let pool = &self.pool;
                async move {
                    let tokens = refresh::ensure_tokens(pool, credential.value(), &ctx, &scope).await?;
                    let access_token = tokens.access_token.clone();
                    let channel = channel_urls(&ctx)
                        .await
                        .map_err(|err| AttemptFailure { passthrough: err, mark: None })?;
                    let url = build_url(Some(channel.api_base.as_str()), &path);
                    let client = shared_client(ctx.proxy.as_deref())?;
                    let req_headers = build_headers(
                        &access_token,
                        headers.anthropic_version,
                        headers.anthropic_beta,
                        ctx.user_agent.as_deref(),
                    )?;
                    let request_headers = headers_to_json(&req_headers);
                    let response = send_with_logging(
                        &ctx,
                        PROVIDER_NAME,
                        "claudecode.models_get",
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
                        operation: "claudecode.models_get".to_string(),
                        model: Some(model),
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

}

#[allow(clippy::result_large_err)]
fn build_headers(
    access_token: &str,
    version: AnthropicVersion,
    beta: Option<AnthropicBetaHeader>,
    user_agent: Option<&str>,
) -> Result<HeaderMap, AttemptFailure> {
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
    let version_value = match version {
        AnthropicVersion::V20230601 => "2023-06-01",
        AnthropicVersion::V20230101 => "2023-01-01",
    };
    headers.insert(HEADER_VERSION, HeaderValue::from_static(version_value));
    let mut values = collect_beta_values(beta);
    ensure_oauth_beta(&mut values);
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
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let ua = resolve_claude_code_ua(user_agent);
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(ua).unwrap_or_else(|_| HeaderValue::from_static(CLAUDE_CODE_UA)),
    );
    Ok(headers)
}

fn resolve_claude_code_ua(user_agent: Option<&str>) -> &str {
    match user_agent {
        Some(value) if is_claude_code_user_agent(value) => value,
        _ => CLAUDE_CODE_UA,
    }
}

fn is_claude_code_user_agent(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("claude-code") || lowered.contains("claude-cli")
}

fn apply_claude_code_system(system: &mut Option<BetaSystemParam>, user_agent: Option<&str>) {
    if user_agent.map(is_claude_code_user_agent).unwrap_or(false) {
        return;
    }

    let prelude = BetaTextBlockParam {
        text: CLAUDE_CODE_SYSTEM_PRELUDE.to_string(),
        r#type: BetaTextBlockType::Text,
        cache_control: None,
        citations: None,
    };

    *system = Some(match system.take() {
        Some(BetaSystemParam::Text(text)) => BetaSystemParam::Blocks(vec![
            prelude,
            BetaTextBlockParam {
                text,
                r#type: BetaTextBlockType::Text,
                cache_control: None,
                citations: None,
            },
        ]),
        Some(BetaSystemParam::Blocks(mut blocks)) => {
            blocks.insert(0, prelude);
            BetaSystemParam::Blocks(blocks)
        }
        None => BetaSystemParam::Blocks(vec![prelude]),
    });
}

fn collect_beta_values(beta: Option<AnthropicBetaHeader>) -> Vec<String> {
    match beta {
        Some(AnthropicBetaHeader::Single(beta)) => vec![match serde_json::to_value(beta) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        }],
        Some(AnthropicBetaHeader::Multiple(list)) => list
            .into_iter()
            .map(|beta| match serde_json::to_value(beta) {
                Ok(JsonValue::String(value)) => value,
                _ => "unknown".to_string(),
            })
            .collect(),
        None => Vec::new(),
    }
}

fn ensure_oauth_beta(values: &mut Vec<String>) {
    if !values.iter().any(|value| value == OAUTH_BETA) {
        values.push(OAUTH_BETA.to_string());
    }
}

#[allow(clippy::result_large_err)]
fn add_beta_value(headers: &mut HeaderMap, value: &str) -> Result<(), AttemptFailure> {
    let mut values: Vec<String> = headers
        .get(HEADER_BETA)
        .and_then(|v| v.to_str().ok())
        .map(|raw| {
            raw.split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if !values.iter().any(|item| item == value) {
        values.push(value.to_string());
    }
    let combined = values.join(",");
    headers.insert(
        HEADER_BETA,
        HeaderValue::from_str(&combined).map_err(|err| AttemptFailure {
            passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
            mark: None,
        })?,
    );
    Ok(())
}

fn is_context_1m_forbidden(error: &UpstreamPassthroughError) -> bool {
    if error.status != http::StatusCode::FORBIDDEN
        && error.status != http::StatusCode::BAD_REQUEST
    {
        return false;
    }
    let Ok(payload) = serde_json::from_slice::<JsonValue>(&error.body) else {
        return false;
    };
    let message = payload
        .get("error")
        .and_then(|err| err.get("message"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    message.contains("the long context beta is not yet available for this subscription.")
        || message.contains(
            "this authentication style is incompatible with the long context beta header.",
        )
}

fn is_sonnet4_model(model: &str) -> bool {
    let lowered = model.to_ascii_lowercase();
    lowered.contains("claude-sonnet-4")
}

fn credential_supports_1m(credential: &BaseCredential) -> Option<bool> {
    credential
        .meta
        .get("claude_1m")
        .and_then(|value| value.as_bool())
}

async fn persist_claude_1m_support(
    pool: &CredentialPool<BaseCredential>,
    ctx: &UpstreamContext,
    credential: &BaseCredential,
    value: bool,
) -> Result<(), UpstreamPassthroughError> {
    let storage = global_storage().ok_or_else(|| {
        UpstreamPassthroughError::service_unavailable("storage unavailable")
    })?;
    let provider_id = match ctx.provider_id {
        Some(id) => id,
        None => {
            let providers = storage
                .list_providers()
                .await
                .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
            providers
                .iter()
                .find(|provider| provider.name == PROVIDER_NAME)
                .map(|provider| provider.id)
                .ok_or_else(|| {
                    UpstreamPassthroughError::service_unavailable("provider not found")
                })?
        }
    };
    let credentials = storage
        .list_credentials()
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;
    let Some(record) = credentials
        .into_iter()
        .find(|item| item.id == credential.id)
    else {
        return Ok(());
    };
    let mut meta = match record.meta_json {
        JsonValue::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    meta.insert("claude_1m".to_string(), JsonValue::Bool(value));
    let meta_json = JsonValue::Object(meta.clone());
    let input = AdminCredentialInput {
        id: Some(record.id),
        provider_id,
        name: record.name.clone(),
        secret: record.secret.clone(),
        meta_json: meta_json.clone(),
        weight: record.weight,
        enabled: record.enabled,
    };
    storage
        .upsert_credential(input)
        .await
        .map_err(|err| UpstreamPassthroughError::service_unavailable(err.to_string()))?;

    let weight = if record.weight >= 0 {
        record.weight as u32
    } else {
        0
    };
    let entry = CredentialEntry::new(
        record.id.to_string(),
        record.enabled,
        weight,
        BaseCredential {
            id: record.id,
            name: record.name,
            secret: record.secret,
            meta: meta_json,
        },
    );
    let snapshot = pool.snapshot();
    let mut credentials = snapshot.credentials.as_ref().clone();
    if let Some(pos) = credentials.iter().position(|item| item.id == entry.id) {
        credentials[pos] = entry;
    } else {
        credentials.push(entry);
    }
    let disallow = snapshot.disallow.as_ref().clone();
    pool.replace_snapshot(PoolSnapshot::new(credentials, disallow));
    Ok(())
}

fn credential_claude_oauth(credential: &BaseCredential) -> Option<&JsonValue> {
    credential
        .secret
        .get("claudeAiOauth")
        .or_else(|| credential.secret.get("claude_ai_oauth"))
}

pub(super) fn credential_access_token(credential: &BaseCredential) -> Option<String> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("accessToken"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(super) fn credential_refresh_token(credential: &BaseCredential) -> Option<String> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("refreshToken"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(super) fn credential_expires_at(credential: &BaseCredential) -> Option<i64> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("expiresAt"))
        .and_then(|value| value.as_i64())
}

#[allow(dead_code)]
pub(super) fn credential_scopes(credential: &BaseCredential) -> Vec<String> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("scopes"))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|v| v.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

#[allow(dead_code)]
pub(super) fn credential_subscription_type(credential: &BaseCredential) -> Option<String> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("subscriptionType"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

#[allow(dead_code)]
pub(super) fn credential_rate_limit_tier(credential: &BaseCredential) -> Option<String> {
    credential_claude_oauth(credential)
        .and_then(|value| value.get("rateLimitTier"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(super) fn credential_session_key(credential: &BaseCredential) -> Option<String> {
    credential
        .secret
        .get("sessionKey")
        .or_else(|| credential.secret.get("session_key"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

#[derive(Debug, Clone)]
pub(super) struct ClaudeCodeUrls {
    pub(super) api_base: String,
    pub(super) claude_ai_base: String,
    pub(super) console_base: String,
}

pub(super) async fn channel_urls(
    ctx: &UpstreamContext,
) -> Result<ClaudeCodeUrls, UpstreamPassthroughError> {
    let mut api_base = DEFAULT_API_BASE_URL.to_string();
    let mut claude_ai_base = DEFAULT_CLAUDE_AI_BASE_URL.to_string();
    let mut console_base = DEFAULT_CONSOLE_BASE_URL.to_string();

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
        {
            if let Some(value) = map.get("base_url").and_then(|v| v.as_str()) {
                api_base = value.to_string();
            }
            if let Some(value) = map.get("claude_ai_base_url").and_then(|v| v.as_str()) {
                claude_ai_base = value.to_string();
            }
            if let Some(value) = map.get("console_base_url").and_then(|v| v.as_str()) {
                console_base = value.to_string();
            }
        }
    }

    Ok(ClaudeCodeUrls {
        api_base: api_base.trim_end_matches('/').to_string(),
        claude_ai_base: claude_ai_base.trim_end_matches('/').to_string(),
        console_base: console_base.trim_end_matches('/').to_string(),
    })
}

fn build_url(base_url: Option<&str>, path: &str) -> String {
    let base = base_url.unwrap_or(DEFAULT_API_BASE_URL).trim_end_matches('/');
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
