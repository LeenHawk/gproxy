//! M2 integration tests: full pipeline::execute against a fake upstream.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::app::snapshot::ControlPlaneSnapshot;
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::{ClientError, ConduitSocket, RespStream, UpstreamClient};
use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::outcome::ResponseBody;
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, openai};
use crate::transform::TransformContext;

/// Captured upstream request (http::Request isn't Clone).
struct Seen {
    uri: String,
    body: Bytes,
    headers: HeaderMap,
}

fn assert_openai_chat_request(seen: &Seen, model: &str, stream: bool) -> Value {
    assert!(
        seen.uri.contains("/v1/chat/completions"),
        "uri: {}",
        seen.uri
    );
    assert!(
        !seen.uri.starts_with("ws://") && !seen.uri.starts_with("wss://"),
        "uri: {}",
        seen.uri
    );
    let up: Value = serde_json::from_slice(&seen.body).unwrap();
    assert_eq!(up["model"], model, "{up}");
    assert_eq!(
        up.get("stream").and_then(Value::as_bool).unwrap_or(false),
        stream,
        "{up}"
    );
    up
}

struct FakeUpstream {
    seen: Mutex<Vec<Seen>>,
    /// canned non-stream response statuses, consumed per call; last repeats
    statuses: Vec<StatusCode>,
    /// canned non-stream response body
    response: Bytes,
    /// canned stream chunks (send_streaming)
    chunks: Vec<Bytes>,
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl UpstreamClient for FakeUpstream {
    async fn send(&self, req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        self.capture(&req);
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        let status = self
            .statuses
            .get(i)
            .or_else(|| self.statuses.last())
            .copied()
            .unwrap_or(StatusCode::OK);
        Ok(http::Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(self.response.clone())
            .expect("response"))
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(StatusCode, HeaderMap, RespStream), ClientError> {
        self.capture(&req);
        let mut h = HeaderMap::new();
        h.insert("content-type", "text/event-stream".parse().unwrap());
        let chunks: Vec<Result<Bytes, ClientError>> = self.chunks.iter().cloned().map(Ok).collect();
        Ok((
            StatusCode::OK,
            h,
            Box::pin(futures_util::stream::iter(chunks)),
        ))
    }

    async fn open_websocket(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        self.capture(&req);
        let i = self.calls.fetch_add(1, Ordering::SeqCst);
        let status = self
            .statuses
            .get(i)
            .or_else(|| self.statuses.last())
            .copied()
            .unwrap_or(StatusCode::OK);
        if !status.is_success() {
            return Err(ClientError::Transport(format!(
                "websocket handshake failed with status {status}"
            )));
        }
        Ok(Box::new(FakeWebSocket {
            messages: Mutex::new(self.websocket_messages()),
        }))
    }
}

impl FakeUpstream {
    fn new(response: Bytes, chunks: Vec<Bytes>) -> Self {
        Self {
            seen: Mutex::new(vec![]),
            statuses: vec![StatusCode::OK],
            response,
            chunks,
            calls: AtomicUsize::new(0),
        }
    }

    fn capture(&self, req: &http::Request<Bytes>) {
        self.seen.lock().unwrap().push(Seen {
            uri: req.uri().to_string(),
            body: req.body().clone(),
            headers: req.headers().clone(),
        });
    }

    fn websocket_messages(&self) -> VecDeque<String> {
        if self.chunks.is_empty() {
            return self
                .response_to_websocket_message()
                .into_iter()
                .collect::<VecDeque<_>>();
        }

        let mut messages = VecDeque::new();
        let mut decoder = crate::transform::common::sse::SseDecoder::new();
        for chunk in &self.chunks {
            for frame in decoder.push(chunk) {
                if let Some(message) = sse_data_to_websocket_message(&frame.data) {
                    messages.push_back(message);
                }
            }
        }
        if let Some(frame) = decoder.finish()
            && let Some(message) = sse_data_to_websocket_message(&frame.data)
        {
            messages.push_back(message);
        }
        messages
    }

    fn response_to_websocket_message(&self) -> Option<String> {
        let value: Value = serde_json::from_slice(&self.response).ok()?;
        let response = chat_response_to_responses(value.clone()).unwrap_or(value);
        Some(
            json!({
                "type": "response.completed",
                "response": response,
            })
            .to_string(),
        )
    }
}

struct FakeWebSocket {
    messages: Mutex<VecDeque<String>>,
}

#[async_trait::async_trait]
impl ConduitSocket for FakeWebSocket {
    async fn send_text(&mut self, _text: String) -> Result<(), ClientError> {
        Ok(())
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        self.messages.lock().unwrap().pop_front().map(Ok)
    }
}

fn sse_data_to_websocket_message(data: &str) -> Option<String> {
    let data = data.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    if value.get("type").is_some() {
        return Some(value.to_string());
    }
    if let Some(response_event) = chat_chunk_to_responses_event(value.clone()) {
        return Some(response_event.to_string());
    }
    Some(value.to_string())
}

fn chat_response_to_responses(value: Value) -> Option<Value> {
    let value = with_chat_usage_total(value);
    if value.get("object").and_then(Value::as_str) != Some("chat.completion") {
        return None;
    }
    let response: openai::ChatCompletionResponse = serde_json::from_value(value).ok()?;
    let ctx = chat_to_responses_ctx();
    let response = crate::transform::generate_content::openai_chat_to_openai_responses::response(
        response, &ctx,
    )
    .ok()?;
    serde_json::to_value(response).ok()
}

fn chat_chunk_to_responses_event(value: Value) -> Option<Value> {
    let value = with_chat_usage_total(value);
    if value.get("object").and_then(Value::as_str) != Some("chat.completion.chunk") {
        return None;
    }
    let chunk: openai::ChatCompletionChunk = serde_json::from_value(value).ok()?;
    let ctx = chat_to_responses_ctx();
    let event = crate::transform::generate_content::openai_chat_to_openai_responses::stream_event(
        chunk, &ctx,
    )
    .ok()?;
    serde_json::to_value(event).ok()
}

fn with_chat_usage_total(mut value: Value) -> Value {
    let Some(usage) = value.get_mut("usage").and_then(Value::as_object_mut) else {
        return value;
    };
    if usage.get("total_tokens").is_some() {
        return value;
    }
    let prompt = usage
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let completion = usage
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    usage.insert("total_tokens".to_owned(), Value::from(prompt + completion));
    value
}

fn chat_to_responses_ctx() -> TransformContext {
    TransformContext::new(
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChatCompletions,
        ),
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    )
}

const BUNDLE: &str = r#"{
  "schema_version": 1,
  "orgs": [{ "id": 1, "name": "default", "enabled": true, "description": null }],
  "users": [
    { "id": 1, "name": "dev", "org_id": 1, "team_id": null, "password": null, "enabled": true, "is_admin": false },
    { "id": 2, "name": "noperm", "org_id": 1, "team_id": null, "password": null, "enabled": true, "is_admin": false }
  ],
  "user_keys": [
    { "id": 1, "user_id": 1, "api_key": "sk-test", "label": null, "enabled": true },
    { "id": 2, "user_id": 2, "api_key": "sk-noperm", "label": null, "enabled": true }
  ],
  "route_permissions": [{ "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "*" }],
  "providers": [
    { "id": 1, "name": "oai", "channel": "openai", "label": null, "settings_json": { "base_url": "http://fake.local" }, "credential_strategy": "round_robin", "proxy_url": null, "tls_fingerprint": null, "enabled": true },
    { "id": 2, "name": "cla", "channel": "claudeapi", "label": null, "settings_json": { "base_url": "http://fake.local" }, "credential_strategy": "round_robin", "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "credentials": [
    { "id": 1, "provider_id": 1, "label": null, "secret_json": { "api_key": "up-key" }, "proxy_url": null, "tls_fingerprint": null, "enabled": true },
    { "id": 2, "provider_id": 2, "label": null, "secret_json": { "api_key": "up-key" }, "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "provider_models": [
    { "id": 1, "provider_id": 1, "model_id": "gpt-test", "display_name": null, "variants_json": ["gpt-test-thinking"], "enabled": true }
  ],
  "routes": [
    { "id": 1, "name": "to-openai", "strategy": "failover", "enabled": true, "description": null },
    { "id": 2, "name": "to-claude", "strategy": "failover", "enabled": true, "description": null }
  ],
  "route_members": [
    { "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-test", "weight": 100, "tier": 0, "enabled": true },
    { "id": 2, "route_id": 2, "provider_id": 2, "upstream_model_id": "claude-test", "weight": 100, "tier": 0, "enabled": true }
  ],
  "aliases": [
    { "id": 1, "provider": "*", "alias": "claude-test", "target": "to-openai", "sort_order": 0, "enabled": true },
    { "id": 2, "provider": "*", "alias": "claude-direct", "target": "to-claude", "sort_order": 1, "enabled": true }
  ],
  "routing_rules": [
    { "id": 1, "provider_id": 1, "operation": "list_models", "kind": "open_ai", "implementation": "local", "dest_operation": null, "dest_kind": null, "sort_order": 0, "enabled": true }
  ],
  "rule_sets": [{ "id": 1, "name": "rs", "enabled": true, "description": null }],
  "rules": [
    { "id": 1, "rule_set_id": 1, "kind": "system_text", "config_json": { "text": "PRELUDE" }, "filter_model_pattern": null, "filter_operation_keys": null, "sort_order": 0, "enabled": true },
    { "id": 2, "rule_set_id": 1, "kind": "header", "config_json": { "name": "anthropic-beta", "value": "context-1m", "mode": "merge" }, "filter_model_pattern": null, "filter_operation_keys": null, "sort_order": 1, "enabled": true }
  ],
  "provider_rule_sets": [{ "id": 1, "provider_id": 2, "rule_set_id": 1, "sort_order": 0, "enabled": true }]
}"#;

async fn state_with(fake: Arc<FakeUpstream>) -> (AppState, tempfile::TempDir) {
    state_with_bundle(fake, BUNDLE).await
}

/// BUNDLE with one top-level array replaced (routing_rules / rate_limits / …).
fn bundle_with(key: &str, rows: Value) -> String {
    let mut v: Value = serde_json::from_str(BUNDLE).expect("bundle json");
    v[key] = rows;
    serde_json::to_string(&v).expect("serialize")
}

async fn state_with_bundle(fake: Arc<FakeUpstream>, bundle: &str) -> (AppState, tempfile::TempDir) {
    state_with_ciphers(
        fake,
        bundle,
        &crate::crypto::NoopCipher,
        Arc::new(crate::crypto::NoopCipher),
    )
    .await
}

/// Crypto-aware variant (M5 envelope tests): the bundle is imported through
/// `import_cipher` while the serving state opens with `state_cipher` — passing
/// different ciphers models a master-key mismatch.
async fn state_with_ciphers(
    fake: Arc<FakeUpstream>,
    bundle: &str,
    import_cipher: &dyn crate::crypto::SecretCipher,
    state_cipher: Arc<dyn crate::crypto::SecretCipher>,
) -> (AppState, tempfile::TempDir) {
    let channels = Arc::new(crate::channel::registry::ChannelRegistry::with_builtin());
    build_state(
        fake,
        bundle,
        import_cipher,
        state_cipher,
        channels,
        crate::config::DEFAULT_MAX_ATTEMPTS,
    )
    .await
}

/// Fully-parameterized state builder shared by the helpers above and the M7a
/// failover tests (which need a custom channel registry and/or a tuned retry
/// budget). Imports `bundle` through `import_cipher`, serves through
/// `state_cipher`, and wires `channels` + `max_attempts`.
async fn build_state(
    fake: Arc<FakeUpstream>,
    bundle: &str,
    import_cipher: &dyn crate::crypto::SecretCipher,
    state_cipher: Arc<dyn crate::crypto::SecretCipher>,
    channels: Arc<crate::channel::registry::ChannelRegistry>,
    max_attempts: u32,
) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
        crate::store::persistence::DbPersistence::connect("sqlite::memory:")
            .await
            .expect("db persistence"),
    );
    crate::app::import::import_bundle(persistence.as_ref(), import_cipher, bundle)
        .await
        .expect("import");
    // Materialize each provider's default routing rules (fill-missing, so the
    // bundle's explicit rules win), matching production's seed-at-creation.
    for p in crate::store::persistence::PersistenceBackend::list_providers(persistence.as_ref())
        .await
        .expect("providers")
    {
        crate::api::routing::seed_default_routing(
            persistence.as_ref(),
            channels.as_ref(),
            p.id,
            false,
        )
        .await
        .expect("seed routing");
    }
    let snapshot = ControlPlaneSnapshot::build(persistence.as_ref(), 1)
        .await
        .expect("snapshot");
    let config = Arc::new(RuntimeConfig {
        host: "127.0.0.1".into(),
        port: 0,
        cache: CacheConfig::Memory,
        persistence: PersistenceConfig::Db {
            dsn: "sqlite::memory:".to_string(),
        },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_channel: "releases".to_string(),
        update_data_dir: dir.path().to_path_buf(),
        cors_origins: Vec::new(),
    });
    let cache: Arc<dyn crate::store::cache::CacheBackend> =
        Arc::new(crate::store::cache::MemoryCache::new());
    let snapshot = Arc::new(arc_swap::ArcSwap::from_pointee(snapshot));
    (
        AppState::new(
            config,
            cache,
            persistence,
            fake,
            snapshot,
            channels,
            state_cipher,
        ),
        dir,
    )
}

fn claude_ctx(model: &str, stream: bool) -> RequestCtx {
    claude_ctx_as("sk-test", model, stream)
}

fn claude_ctx_as(api_key: &str, model: &str, stream: bool) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {api_key}").parse().unwrap(),
    );
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model": model,
        "max_tokens": 32,
        "stream": stream,
        "messages": [{ "role": "user", "content": "hi" }]
    });
    RequestCtx {
        request_id: "t-1".into(),
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
        mode: RoutingMode::Aggregated,
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    }
}

mod aggregate;
mod authz;
mod billing;
mod conversion;
mod envelope;
mod health;
mod local;
mod refresh;
mod routing;
mod synthetic;
