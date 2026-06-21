//! M2 integration tests: full pipeline::execute against a fake upstream.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use crate::app::AppState;
use crate::app::snapshot::ControlPlaneSnapshot;
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::{ClientError, RespStream, UpstreamClient};
use crate::pipeline::context::{RequestCtx, RoutingMode};
use crate::pipeline::outcome::ResponseBody;

/// Captured upstream request (http::Request isn't Clone).
struct Seen {
    uri: String,
    body: Bytes,
    headers: HeaderMap,
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
    { "id": 1, "provider_id": 1, "model_id": "gpt-test", "display_name": null, "pricing_json": null, "variants_json": ["-thinking"], "enabled": true }
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
    { "id": 1, "route_id": 1, "alias": "claude-test" },
    { "id": 2, "route_id": 2, "alias": "claude-direct" }
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
        crate::store::persistence::FilePersistence::open(dir.path().to_path_buf())
            .await
            .expect("file persistence"),
    );
    crate::app::import::import_bundle(persistence.as_ref(), import_cipher, bundle)
        .await
        .expect("import");
    // Materialize each provider's default routing rules (fill-missing, so the
    // bundle's explicit rules win), matching production's seed-at-creation.
    for p in persistence.list_providers().await.expect("providers") {
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
        persistence: PersistenceConfig::File {
            data_dir: dir.path().to_path_buf(),
        },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_repo: None,
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

#[tokio::test]
async fn claude_inbound_to_openai_buffered() {
    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-test", false))
        .await
        .expect("pipeline ok");

    // upstream saw the TARGET protocol
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen[0].uri.contains("/v1/chat/completions"),
        "uri: {}",
        seen[0].uri
    );
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(up["model"], "gpt-test"); // member model rewrite
    drop(seen);

    // client got CLAUDE shape back
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected Full")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["role"], "assistant");
    assert_eq!(v["content"][0]["text"], "hello");
    assert_eq!(outcome.status, StatusCode::OK);
}

#[tokio::test]
async fn claude_inbound_to_openai_streaming() {
    let c1 = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"role":"assistant","content":"he"},"finish_reason":null}]}"#;
    let c2 = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"llo"},"finish_reason":null}]}"#;
    let c3 = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("{c1}\n\n")),
            Bytes::from(format!("{c2}\n\n{c3}\n\n")),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-test", true))
        .await
        .expect("pipeline ok");

    let ResponseBody::Stream(s) = outcome.body else {
        panic!("expected Stream")
    };
    use futures_util::StreamExt;
    let collected: Vec<Bytes> = s.map(|r| r.expect("chunk ok")).collect().await;
    let text = String::from_utf8(collected.concat()).unwrap();
    assert!(
        text.contains("event: "),
        "claude SSE has event names: {text}"
    );
    assert!(!text.contains("[DONE]"), "no DONE in claude stream: {text}");
    let seen = fake.seen.lock().unwrap();
    assert!(seen[0].uri.contains("/v1/chat/completions"));
}

#[tokio::test]
async fn gemini_inbound_streaming_sets_body_stream_flag() {
    let c1 = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"role":"assistant","content":"hi"},"finish_reason":null}]}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("{c1}\n\n")),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({ "contents": [{ "role": "user", "parts": [{ "text": "hi" }] }] });
    let ctx = RequestCtx {
        request_id: "t-g".into(),
        method: Method::POST,
        path: "/v1beta/models/gemini-pro:streamGenerateContent".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
        mode: RoutingMode::Scoped {
            provider: "oai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");

    // upstream must be asked to STREAM in the body (gemini carried it in the URL)
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen[0].uri.contains("/v1/chat/completions"),
        "uri: {}",
        seen[0].uri
    );
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(up["stream"], true, "stream flag injected: {up}");
    drop(seen);
    let ResponseBody::Stream(_) = outcome.body else {
        panic!("expected Stream")
    };
}

#[tokio::test]
async fn process_rules_apply_on_claude_passthrough() {
    let msg_response = json!({
        "id": "msg-1", "type": "message", "role": "assistant", "model": "claude-test",
        "content": [{ "type": "text", "text": "ok" }],
        "stop_reason": "end_turn", "stop_sequence": null,
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&msg_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-direct", false))
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    let seen = fake.seen.lock().unwrap();
    assert!(seen[0].uri.contains("/v1/messages"), "passthrough path");
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    // claudeapi's shape_request sanitizes the body: the system_text rule's
    // "PRELUDE" string is canonicalized to the block-array form.
    assert_eq!(up["system"][0]["text"], "PRELUDE", "system_text applied");
    assert_eq!(up["model"], "claude-test");
    assert_eq!(
        seen[0].headers.get("anthropic-beta").unwrap(),
        "context-1m",
        "header rule forwarded (claudeapi whitelists it)"
    );
}

#[tokio::test]
async fn scoped_variant_suffix_strips_to_base() {
    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "hi" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model": "gpt-test-thinking",
        "messages": [{ "role": "user", "content": "hi" }]
    });
    let ctx = RequestCtx {
        request_id: "t-v".into(),
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
        mode: RoutingMode::Scoped {
            provider: "oai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx).await.expect("ok");
    assert_eq!(outcome.status, StatusCode::OK);
    let seen = fake.seen.lock().unwrap();
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(up["model"], "gpt-test", "variant suffix stripped to base");
}

mod aggregate;
mod authz;
mod billing;
mod envelope;
mod health;
mod local;
mod refresh;
