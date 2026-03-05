use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::{Body, Bytes, to_bytes};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::from_fn;
use axum::response::Response;
use axum::routing::{get, post};
use futures_util::Stream;
use gproxy_middleware::{
    MiddlewareTransformError, OperationFamily, ProtocolKind, TransformRequest,
    TransformRequestPayload, TransformResponsePayload, UsageSnapshot, attach_usage_extractor,
};
use gproxy_protocol::claude::count_tokens::request as claude_count_tokens_request;
use gproxy_protocol::claude::count_tokens::response as claude_count_tokens_response;
use gproxy_protocol::claude::create_message::response as claude_create_message_response;
use gproxy_protocol::claude::create_message::types::{BetaUsage, Model as ClaudeModel};
use gproxy_protocol::claude::model_list::request as claude_model_list_request;
use gproxy_protocol::gemini::count_tokens::request as gemini_count_tokens_request;
use gproxy_protocol::gemini::count_tokens::response as gemini_count_tokens_response;
use gproxy_protocol::gemini::generate_content::response as gemini_generate_content_response;
use gproxy_protocol::gemini::generate_content::types::GeminiUsageMetadata;
use gproxy_protocol::gemini::model_list::request as gemini_model_list_request;
use gproxy_protocol::openai::compact_response::response as openai_compact_response_response;
use gproxy_protocol::openai::compact_response::types::ResponseUsage as CompactResponseUsage;
use gproxy_protocol::openai::count_tokens::request as openai_count_tokens_request;
use gproxy_protocol::openai::count_tokens::response as openai_count_tokens_response;
use gproxy_protocol::openai::count_tokens::types::ResponseInput;
use gproxy_protocol::openai::create_chat_completions::response as openai_chat_completions_response;
use gproxy_protocol::openai::create_chat_completions::types::CompletionUsage;
use gproxy_protocol::openai::create_response::response as openai_create_response_response;
use gproxy_protocol::openai::create_response::types::ResponseUsage;
use gproxy_protocol::openai::embeddings::response as openai_embeddings_response;
use gproxy_protocol::openai::embeddings::types::OpenAiEmbeddingModel;
use gproxy_protocol::openai::embeddings::types::OpenAiEmbeddingUsage;
use gproxy_protocol::openai::model_list::request as openai_model_list_request;
use gproxy_protocol::stream::SseToNdjsonRewriter;
use serde_json::json;
use tokio::sync::mpsc;

use gproxy_provider::{
    BuiltinChannel, ChannelId, CredentialRef, ProviderDefinition, RetryWithPayloadRequest,
    RouteImplementation, RouteKey, TokenizerResolutionContext, TrackedHttpEvent,
    UpstreamCredentialUpdate, UpstreamError, UpstreamOAuthResponse, UpstreamRequestMeta,
    UpstreamResponse, capture_tracked_http_events, credential_kind_for_storage, parse_query_value,
    try_local_response_for_channel,
};
use gproxy_storage::{
    CredentialQuery, CredentialStatusWrite, CredentialWrite, ProviderQuery, ProviderWrite, Scope,
    StorageWriteBatch, StorageWriteEvent, StorageWriteSink, UpstreamRequestWrite, UsageWrite,
};

use crate::AppState;

use super::error::HttpError;

mod handlers;
use handlers::*;
mod auth;
use auth::*;
mod model_prefix;
use model_prefix::*;
mod recording;
use recording::*;
mod execute;
use execute::*;

const X_API_KEY: &str = "x-api-key";
const X_GOOG_API_KEY: &str = "x-goog-api-key";
const AUTHORIZATION: &str = "authorization";
const CLAUDE_ANTHROPIC_VERSION_HEADER: &str = "anthropic-version";
const CLAUDE_ANTHROPIC_BETA_HEADER: &str = "anthropic-beta";
const BODY_CAPTURE_LIMIT_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelProtocolPreference {
    OpenAi,
    Claude,
    Gemini,
}

pub(super) fn model_protocol_preference(
    headers: &HeaderMap,
    raw_query: Option<&str>,
) -> ModelProtocolPreference {
    let has_gemini_auth = has_gemini_model_auth(headers, raw_query);
    if has_gemini_auth {
        return ModelProtocolPreference::Gemini;
    }
    let has_bearer = header_value(headers, AUTHORIZATION).is_some();
    if has_bearer && headers.contains_key(CLAUDE_ANTHROPIC_VERSION_HEADER) {
        return ModelProtocolPreference::Claude;
    }
    if has_bearer {
        return ModelProtocolPreference::OpenAi;
    }
    ModelProtocolPreference::OpenAi
}

pub(super) fn has_gemini_model_auth(headers: &HeaderMap, raw_query: Option<&str>) -> bool {
    headers.contains_key(X_GOOG_API_KEY) || parse_query_value(raw_query, "key").is_some()
}

#[derive(Debug, Clone, Copy)]
struct RequestAuthContext {
    user_id: i64,
    user_key_id: i64,
    downstream_trace_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct UsageRequestContext {
    operation: OperationFamily,
    protocol: ProtocolKind,
    model: Option<String>,
    body_for_estimate: Option<Vec<u8>>,
}

impl UsageRequestContext {
    const fn operation(&self) -> OperationFamily {
        self.operation
    }

    const fn protocol(&self) -> ProtocolKind {
        self.protocol
    }
}

#[derive(Clone)]
struct UpstreamStreamRecordContext {
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    request: UsageRequestContext,
    provider_id: Option<i64>,
    credential_id: Option<i64>,
    request_meta: Option<UpstreamRequestMeta>,
    response_status: Option<u16>,
    response_headers: Vec<(String, String)>,
    stream_usage: Option<gproxy_middleware::UsageHandle>,
    record_upstream_event: bool,
    record_stream_usage_event: bool,
}

#[derive(Default)]
struct UpstreamStreamRecordState {
    captured: Vec<u8>,
    capture_truncated: bool,
    flushed: bool,
}

struct UpstreamStreamRecordGuard {
    context: UpstreamStreamRecordContext,
    state: Arc<Mutex<UpstreamStreamRecordState>>,
}

impl UpstreamStreamRecordGuard {
    fn new(context: UpstreamStreamRecordContext) -> Self {
        Self {
            context,
            state: Arc::new(Mutex::new(UpstreamStreamRecordState::default())),
        }
    }

    fn push_chunk(&self, chunk: &[u8]) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.capture_truncated {
            return;
        }
        let remaining = BODY_CAPTURE_LIMIT_BYTES.saturating_sub(state.captured.len());
        if remaining > 0 {
            let take = chunk.len().min(remaining);
            state.captured.extend_from_slice(&chunk[..take]);
        }
        if state.captured.len() >= BODY_CAPTURE_LIMIT_BYTES {
            state.capture_truncated = true;
        }
    }

    fn take_flush_payload(&self) -> Option<(UpstreamStreamRecordContext, Option<Vec<u8>>)> {
        let Ok(mut state) = self.state.lock() else {
            return None;
        };
        if state.flushed {
            return None;
        }
        state.flushed = true;
        let response_body =
            (!state.captured.is_empty()).then(|| std::mem::take(&mut state.captured));
        Some((self.context.clone(), response_body))
    }

    async fn flush_now(&self) {
        if let Some((context, response_body)) = self.take_flush_payload() {
            let response_body_for_usage = response_body.clone();
            if context.record_upstream_event {
                enqueue_upstream_request_event_from_meta(
                    context.state.as_ref(),
                    context.auth.downstream_trace_id,
                    context.provider_id,
                    context.credential_id,
                    context.request_meta.as_ref(),
                    UpstreamResponseMeta {
                        status: context.response_status,
                        headers: context.response_headers.as_slice(),
                        body: response_body,
                    },
                )
                .await;
            }
            if context.record_stream_usage_event {
                let stream_usage = context
                    .stream_usage
                    .as_ref()
                    .and_then(|handle| handle.latest());
                enqueue_stream_usage_event_with_estimate(
                    &context,
                    response_body_for_usage.as_deref().unwrap_or(&[]),
                    stream_usage,
                )
                .await;
            }
        }
    }
}

impl Drop for UpstreamStreamRecordGuard {
    fn drop(&mut self) {
        let Some((context, response_body)) = self.take_flush_payload() else {
            return;
        };
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        handle.spawn(async move {
            let response_body_for_usage = response_body.clone();
            if context.record_upstream_event {
                enqueue_upstream_request_event_from_meta(
                    context.state.as_ref(),
                    context.auth.downstream_trace_id,
                    context.provider_id,
                    context.credential_id,
                    context.request_meta.as_ref(),
                    UpstreamResponseMeta {
                        status: context.response_status,
                        headers: context.response_headers.as_slice(),
                        body: response_body,
                    },
                )
                .await;
            }
            if context.record_stream_usage_event {
                let stream_usage = context
                    .stream_usage
                    .as_ref()
                    .and_then(|handle| handle.latest());
                enqueue_stream_usage_event_with_estimate(
                    &context,
                    response_body_for_usage.as_deref().unwrap_or(&[]),
                    stream_usage,
                )
                .await;
            }
        });
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/messages", post(claude_messages_unscoped))
        .route(
            "/v1/messages/count_tokens",
            post(claude_count_tokens_unscoped),
        )
        .route(
            "/v1/chat/completions",
            post(openai_chat_completions_unscoped),
        )
        .route(
            "/v1/responses",
            post(openai_responses_unscoped).get(openai_responses_upgrade_unscoped),
        )
        .route(
            "/v1/responses/input_tokens",
            post(openai_input_tokens_unscoped),
        )
        .route("/v1/embeddings", post(openai_embeddings_unscoped))
        .route("/v1/responses/compact", post(openai_compact_unscoped))
        .route("/v1/models", get(v1_model_list_unscoped))
        .route("/v1/models/{*model_id}", get(v1_model_get_unscoped))
        .route("/v1beta/models", get(v1beta_model_list_unscoped))
        .route("/v1beta/{*target}", get(v1beta_model_get_unscoped))
        .route("/v1beta/{*target}", post(v1beta_post_target_unscoped))
        .route("/v1/{*target}", post(v1_post_target_unscoped))
        .route("/{provider}/v1/oauth", get(oauth_start))
        .route("/{provider}/v1/oauth/callback", get(oauth_callback))
        .route("/{provider}/v1/usage", get(upstream_usage))
        .route("/{provider}/v1/realtime", get(openai_realtime_upgrade))
        .route(
            "/{provider}/v1/realtime/{*tail}",
            get(openai_realtime_upgrade_with_tail),
        )
        .route("/{provider}/v1/messages", post(claude_messages))
        .route(
            "/{provider}/v1/messages/count_tokens",
            post(claude_count_tokens),
        )
        .route(
            "/{provider}/v1/chat/completions",
            post(openai_chat_completions),
        )
        .route(
            "/{provider}/v1/responses",
            post(openai_responses).get(openai_responses_upgrade),
        )
        .route(
            "/{provider}/v1/responses/input_tokens",
            post(openai_input_tokens),
        )
        .route("/{provider}/v1/embeddings", post(openai_embeddings))
        .route("/{provider}/v1/responses/compact", post(openai_compact))
        .route("/{provider}/v1/models", get(v1_model_list))
        .route("/{provider}/v1/models/{*model_id}", get(v1_model_get))
        .route("/{provider}/v1beta/models", get(v1beta_model_list))
        .route("/{provider}/v1beta/{*target}", get(v1beta_model_get))
        .route("/{provider}/v1beta/{*target}", post(v1beta_post_target))
        .route("/{provider}/v1/{*target}", post(v1_post_target))
        .layer(from_fn(normalize_provider_auth_header))
}

fn oauth_response_to_axum(response: UpstreamOAuthResponse) -> Response {
    let status = StatusCode::from_u16(response.status_code).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut builder = Response::builder().status(status);
    for (name, value) in response.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(Body::from(response.body))
        .unwrap_or_else(|_| Response::new(Body::from("failed to build provider response")))
}

fn websocket_upgrade_required_response(message: &str) -> Response {
    let body = serde_json::to_vec(&json!({
        "error": {
            "message": message,
            "type": "upgrade_required",
            "code": "websocket_upgrade_required"
        }
    }))
    .unwrap_or_default();

    Response::builder()
        .status(StatusCode::UPGRADE_REQUIRED)
        .header("content-type", "application/json")
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::from("failed to build websocket upgrade response")))
}

fn oauth_callback_response_to_axum(
    result: gproxy_provider::UpstreamOAuthCallbackResult,
    credential_id: Option<i64>,
) -> Response {
    let upstream = serde_json::from_slice::<serde_json::Value>(&result.response.body)
        .unwrap_or_else(|_| {
            serde_json::Value::String(String::from_utf8_lossy(&result.response.body).to_string())
        });
    let body = serde_json::to_vec(&json!({
        "upstream": upstream,
        "credential": result.credential,
        "credential_id": credential_id,
    }))
    .unwrap_or_default();

    let mut headers = result.response.headers;
    headers.retain(|(name, _)| !name.eq_ignore_ascii_case("content-type"));
    headers.push(("content-type".to_string(), "application/json".to_string()));

    oauth_response_to_axum(UpstreamOAuthResponse {
        status_code: result.response.status_code,
        headers,
        body,
        request_meta: result.response.request_meta,
    })
}

fn bad_request(message: impl Into<String>) -> HttpError {
    HttpError::new(StatusCode::BAD_REQUEST, message)
}

fn internal_error(message: impl Into<String>) -> HttpError {
    HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, message)
}

fn parse_optional_query_value<T>(query: Option<&str>, key: &str) -> Result<Option<T>, HttpError>
where
    T: FromStr,
{
    let Some(raw) = parse_query_value(query, key) else {
        return Ok(None);
    };
    raw.parse::<T>()
        .map(Some)
        .map_err(|_| bad_request(format!("invalid query parameter `{key}`: {raw}")))
}

async fn resolve_provider_id(state: &AppState, channel: &ChannelId) -> Result<i64, HttpError> {
    let storage = state.load_storage();
    let rows = storage
        .list_providers(&ProviderQuery {
            channel: Scope::Eq(channel.as_str().to_string()),
            name: Scope::All,
            enabled: Scope::All,
            limit: Some(1),
        })
        .await
        .map_err(|err| internal_error(err.to_string()))?;
    if let Some(row) = rows.into_iter().next() {
        return Ok(row.id);
    }

    let provider = state
        .config
        .load()
        .providers
        .get(channel)
        .cloned()
        .ok_or_else(|| {
            internal_error(format!("provider {} not found in config", channel.as_str()))
        })?;
    let provider_settings_json =
        gproxy_provider::provider_settings_to_json_string_with_credential_pick_mode(
            &provider.settings,
            provider.credential_pick_mode,
        )
        .map_err(|err| internal_error(err.to_string()))?;
    let provider_dispatch_json =
        serde_json::to_string(&provider.dispatch).map_err(|err| internal_error(err.to_string()))?;
    storage
        .create_provider(
            channel.as_str(),
            channel.as_str(),
            provider_settings_json.as_str(),
            provider_dispatch_json.as_str(),
            true,
        )
        .await
        .map_err(|err| internal_error(err.to_string()))
}

async fn resolve_credential_id(
    state: &AppState,
    provider_id: i64,
    credential: &CredentialRef,
) -> Result<i64, HttpError> {
    let storage = state.load_storage();
    let expected_name = credential
        .label
        .clone()
        .unwrap_or_else(|| credential.id.to_string());
    let rows = storage
        .list_credentials(&CredentialQuery {
            provider_id: Scope::Eq(provider_id),
            kind: Scope::All,
            enabled: Scope::All,
            limit: Some(256),
        })
        .await
        .map_err(|err| internal_error(err.to_string()))?;

    if let Some(row) = rows
        .into_iter()
        .find(|row| row.name.as_deref() == Some(expected_name.as_str()))
    {
        return Ok(row.id);
    }

    let credential_secret_json = serde_json::to_string(&credential.credential)
        .map_err(|err| internal_error(err.to_string()))?;
    storage
        .create_credential(
            provider_id,
            Some(expected_name.as_str()),
            credential_kind_for_storage(&credential.credential).as_str(),
            None,
            credential_secret_json.as_str(),
            true,
        )
        .await
        .map_err(|err| internal_error(err.to_string()))
}

async fn persist_provider_and_credential(
    state: &AppState,
    channel: &ChannelId,
    provider: &ProviderDefinition,
    credential: &CredentialRef,
) -> Result<(), HttpError> {
    let provider_id = resolve_provider_id(state, channel).await?;
    let provider_settings_json =
        gproxy_provider::provider_settings_to_json_string_with_credential_pick_mode(
            &provider.settings,
            provider.credential_pick_mode,
        )
        .map_err(|err| internal_error(err.to_string()))?;
    let provider_dispatch_json =
        serde_json::to_string(&provider.dispatch).map_err(|err| internal_error(err.to_string()))?;
    let provider_write = ProviderWrite {
        id: provider_id,
        name: channel.as_str().to_string(),
        channel: channel.as_str().to_string(),
        settings_json: provider_settings_json,
        dispatch_json: provider_dispatch_json,
        enabled: true,
    };
    let credential_id = resolve_credential_id(state, provider_id, credential).await?;
    let credential_secret_json = serde_json::to_string(&credential.credential)
        .map_err(|err| internal_error(err.to_string()))?;
    let credential_write = CredentialWrite {
        id: credential_id,
        provider_id,
        name: credential
            .label
            .clone()
            .or_else(|| Some(credential.id.to_string())),
        kind: credential_kind_for_storage(&credential.credential),
        settings_json: None,
        secret_json: credential_secret_json,
        enabled: true,
    };
    let mut batch = StorageWriteBatch::default();
    batch.apply(StorageWriteEvent::UpsertProvider(provider_write));
    batch.apply(StorageWriteEvent::UpsertCredential(credential_write));
    state
        .load_storage()
        .write_batch(batch)
        .await
        .map_err(|err| internal_error(err.to_string()))
}

async fn apply_credential_update_and_persist(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    update: UpstreamCredentialUpdate,
) {
    if !state.apply_upstream_credential_update_in_memory(&channel, &update) {
        eprintln!(
            "provider: skip credential update, in-memory apply failed channel={} credential_id={}",
            channel.as_str(),
            update.credential_id()
        );
        return;
    }
    let Some(credential) =
        state.get_provider_credential_in_memory(&channel, update.credential_id())
    else {
        eprintln!(
            "provider: skip credential update, updated credential missing in-memory channel={} credential_id={}",
            channel.as_str(),
            update.credential_id()
        );
        return;
    };

    if let Err(err) =
        persist_provider_and_credential(&state, &channel, &provider, &credential).await
    {
        eprintln!(
            "provider: persist credential update failed channel={} credential_id={} error={:?}",
            channel.as_str(),
            credential.id,
            err
        );
    }
}

fn collect_model_ids_from_response_json(value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(items) = value.get("data").and_then(serde_json::Value::as_array) {
        for item in items {
            if let Some(id) = item.get("id").and_then(serde_json::Value::as_str) {
                let model_id = normalize_unscoped_model_id(id);
                if !model_id.is_empty() {
                    out.push(model_id);
                }
            }
        }
    }

    if let Some(items) = value.get("models").and_then(serde_json::Value::as_array) {
        for item in items {
            let name = item
                .get("name")
                .and_then(serde_json::Value::as_str)
                .or_else(|| item.get("id").and_then(serde_json::Value::as_str));
            if let Some(name) = name {
                let model_id = normalize_unscoped_model_id(name);
                if !model_id.is_empty() {
                    out.push(model_id);
                }
            }
        }
    }
}

async fn collect_provider_model_ids(
    state: Arc<AppState>,
    channel: ChannelId,
    provider: ProviderDefinition,
    auth: RequestAuthContext,
    headers: &HeaderMap,
) -> Vec<String> {
    let channel_prefix = channel.as_str().to_string();
    let mut openai = openai_model_list_request::OpenAiModelListRequest::default();

    let mut claude = claude_model_list_request::ClaudeModelListRequest::default();
    let (version, beta) = anthropic_headers_from_request(headers);
    claude.headers.anthropic_version = version;
    if beta.is_some() {
        claude.headers.anthropic_beta = beta;
    }

    let gemini = gemini_model_list_request::GeminiModelListRequest::default();
    openai.query = openai_model_list_request::QueryParameters::default();
    let candidates = match model_protocol_preference(headers, None) {
        ModelProtocolPreference::Claude => vec![
            TransformRequest::ModelListClaude(claude),
            TransformRequest::ModelListOpenAi(openai),
            TransformRequest::ModelListGemini(gemini),
        ],
        ModelProtocolPreference::Gemini => vec![TransformRequest::ModelListGemini(gemini)],
        ModelProtocolPreference::OpenAi => vec![
            TransformRequest::ModelListOpenAi(openai),
            TransformRequest::ModelListClaude(claude),
            TransformRequest::ModelListGemini(gemini),
        ],
    };

    let response =
        match execute_transform_candidates(state, channel, provider, auth, candidates).await {
            Ok(response) => response,
            Err(_) => return Vec::new(),
        };

    if !response.status().is_success() {
        return Vec::new();
    }

    let body = match to_bytes(response.into_body(), BODY_CAPTURE_LIMIT_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };
    let value = match serde_json::from_slice::<serde_json::Value>(&body) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut ids = Vec::new();
    collect_model_ids_from_response_json(&value, &mut ids);
    ids.into_iter()
        .map(|model_id| format!("{channel_prefix}/{model_id}"))
        .collect()
}

async fn collect_unscoped_model_ids(
    state: Arc<AppState>,
    auth: RequestAuthContext,
    headers: &HeaderMap,
) -> Vec<String> {
    let providers: Vec<(ChannelId, ProviderDefinition)> = {
        let snapshot = state.config.load();
        snapshot
            .providers
            .providers
            .iter()
            .map(|provider| (provider.channel.clone(), provider.clone()))
            .collect()
    };

    let mut ids = Vec::new();
    for (channel, provider) in providers {
        ids.extend(
            collect_provider_model_ids(state.clone(), channel, provider, auth, headers).await,
        );
    }

    let mut dedup = std::collections::BTreeSet::new();
    ids.retain(|id| dedup.insert(id.clone()));
    ids
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    use super::{ModelProtocolPreference, model_protocol_preference};

    fn headers(values: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in values {
            let header_name = HeaderName::from_bytes(name.as_bytes()).expect("valid header name");
            headers.insert(
                header_name,
                HeaderValue::from_str(value).expect("valid header value"),
            );
        }
        headers
    }

    #[test]
    fn model_list_prefers_openai_for_x_api_key_without_bearer() {
        let headers = headers(&[("x-api-key", "test")]);
        assert_eq!(
            model_protocol_preference(&headers, None),
            ModelProtocolPreference::OpenAi
        );
    }

    #[test]
    fn model_list_prefers_openai_for_anthropic_version_without_bearer() {
        let headers = headers(&[("anthropic-version", "2023-06-01")]);
        assert_eq!(
            model_protocol_preference(&headers, None),
            ModelProtocolPreference::OpenAi
        );
    }

    #[test]
    fn model_list_prefers_claude_for_anthropic_version_even_with_bearer() {
        let headers = headers(&[
            ("anthropic-version", "2023-06-01"),
            ("authorization", "Bearer test"),
        ]);
        assert_eq!(
            model_protocol_preference(&headers, None),
            ModelProtocolPreference::Claude
        );
    }

    #[test]
    fn model_list_prefers_gemini_for_query_key() {
        let headers = HeaderMap::new();
        assert_eq!(
            model_protocol_preference(&headers, Some("key=test")),
            ModelProtocolPreference::Gemini
        );
    }

    #[test]
    fn model_list_prefers_gemini_for_x_goog_api_key() {
        let headers = headers(&[("x-goog-api-key", "test")]);
        assert_eq!(
            model_protocol_preference(&headers, None),
            ModelProtocolPreference::Gemini
        );
    }

    #[test]
    fn model_list_uses_openai_for_bearer_by_default() {
        let headers = headers(&[("authorization", "Bearer test")]);
        assert_eq!(
            model_protocol_preference(&headers, None),
            ModelProtocolPreference::OpenAi
        );
    }
}
