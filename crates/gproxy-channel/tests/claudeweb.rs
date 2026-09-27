#![cfg(feature = "claudeweb")]

mod support;

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        ChannelState, CredentialContext, CredentialView, LoginContext, OperationContext,
        ProviderView, QuotaScope, QuotaValue,
    },
    channels::claudeweb::{ClaudeWeb, SessionState, continuation_key},
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityFuture, CapabilityLimits, CasResult,
        StateEntry, StateWrite, Version,
    },
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
};

type Sent = (Method, String, HeaderMap, Vec<u8>);

struct ScriptClient {
    replies: Mutex<VecDeque<WireResponse>>,
    requests: Mutex<Vec<Sent>>,
}
impl ScriptClient {
    fn new(replies: Vec<WireResponse>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        })
    }
    fn sent(&self) -> Vec<Sent> {
        self.requests.lock().unwrap().clone()
    }
    fn push(&self, reply: WireResponse) {
        self.replies.lock().unwrap().push_back(reply);
    }
}
impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = match body {
                HttpBody::Bytes(bytes) => bytes.to_vec(),
                HttpBody::Stream(_) => Vec::new(),
            };
            self.requests.lock().unwrap().push((
                parts.method,
                parts.uri.to_string(),
                parts.headers,
                body,
            ));
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected upstream call"))
        })
    }
}

type Entry = (Bytes, Version, Option<SystemTime>);

/// A tiny CAS store: expired entries read as absent, versions never repeat.
#[derive(Default)]
struct MemoryState {
    entries: Mutex<HashMap<String, Entry>>,
    counter: AtomicU64,
}
impl MemoryState {
    fn new() -> Arc<Self> {
        Arc::default()
    }
    fn keys(&self) -> Vec<String> {
        self.entries.lock().unwrap().keys().cloned().collect()
    }
}
impl ChannelState for MemoryState {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move {
            let entries = self.entries.lock().unwrap();
            Ok(entries
                .get(key)
                .filter(|(_, _, expires)| expires.is_none_or(|at| at > SystemTime::now()))
                .map(|(payload, version, expires_at)| StateEntry {
                    payload: payload.clone(),
                    version: version.clone(),
                    expires_at: *expires_at,
                }))
        })
    }
    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let mut entries = self.entries.lock().unwrap();
            let current = entries.get(key).map(|(_, version, _)| version.clone());
            if current != expected {
                return Ok(CasResult::Conflict);
            }
            match replacement {
                Some(write) => {
                    let version = Version::from_bytes(
                        self.counter
                            .fetch_add(1, Ordering::SeqCst)
                            .to_be_bytes()
                            .to_vec(),
                    );
                    entries.insert(
                        key.to_owned(),
                        (write.payload, version.clone(), write.expires_at),
                    );
                    Ok(CasResult::Applied(Some(version)))
                }
                None => {
                    entries.remove(key);
                    Ok(CasResult::Applied(None))
                }
            }
        })
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(600),
            stream_idle: Duration::from_secs(60),
            read_bytes: 0,
            write_bytes: 0,
            ws_frame_bytes: 0,
        }
    }
}

fn reply(status: StatusCode, value: Value) -> WireResponse {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}

fn sse(status: StatusCode, body: &'static str) -> WireResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    WireResponse {
        status,
        headers,
        body: HttpBody::Bytes(Bytes::from_static(body.as_bytes())),
    }
}

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "claudeweb-1",
        channel: "claudeweb",
        base_url,
        config,
    }
}

fn secret() -> Value {
    json!({
        "cookie": "sessionKey=sk-ant-sid01-example",
        "organization_uuid": "org-1",
        "device_id": "dev-1",
        "capabilities": ["chat", "claude_pro"]
    })
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "cred-1",
        provider_id: "claudeweb-1",
        auth_kind: "cookie",
        secret,
        metadata,
        version: 1,
        expires_at_ms: None,
    }
}

fn context<'a>(
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    client: Arc<ScriptClient>,
    state: Arc<MemoryState>,
    headers: HeaderMap,
    body: Value,
) -> OperationContext<'a> {
    OperationContext {
        provider,
        credential,
        dialect: Dialect::Claude,
        request: WireRequest {
            method: Method::POST,
            path: "/v1/messages".into(),
            query: None,
            headers,
            body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        },
        client,
        state,
        instance_id: Arc::from("local"),
        endpoint_override: None,
    }
}

async fn drain(response: WireResponse) -> String {
    match response.body {
        HttpBody::Bytes(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            String::from_utf8(out).unwrap()
        }
    }
}

fn events(sse: &str) -> Vec<Value> {
    sse.split("\n\n")
        .filter_map(|block| block.lines().find_map(|line| line.strip_prefix("data: ")))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect()
}

const TEXT_STREAM: &str = "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-up\",\"content\":[]}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello there\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null}}\n\ndata: {\"type\":\"message_stop\"}\n\n";

#[test]
fn default_connection_is_a_chrome_preset_over_wreq() {
    use gproxy_client::{Backend, EmulationConfig};
    let config = ClaudeWeb::new()
        .default_connection()
        .expect("channel default");
    assert_eq!(config.backend, Backend::Wreq);
    assert!(config.gzip && config.brotli && config.deflate && config.zstd);
    let Some(EmulationConfig::Preset {
        profile,
        platform,
        http2,
        headers,
    }) = &config.emulation
    else {
        panic!("preset expected: {:?}", config.emulation);
    };
    assert_eq!(profile, "chrome_149");
    assert!(["linux", "macos", "windows"].contains(&platform.as_str()));
    assert!(*http2 && *headers);
}

#[tokio::test]
async fn new_turn_uploads_creates_configures_completes_and_deletes() {
    let config = json!({"prompt": "be brief", "timezone": "Europe/Berlin"});
    let secret = secret();
    let metadata = json!({"pro": true});
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({"file_uuid": "file-1"})),
        reply(StatusCode::CREATED, json!({"uuid": "ignored"})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TEXT_STREAM),
        reply(StatusCode::NO_CONTENT, json!({})),
    ]);
    let state = MemoryState::new();
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_static("client-key"));
    headers.insert("cookie", HeaderValue::from_static("sessionKey=spoof"));
    headers.insert("x-client-trace", HeaderValue::from_static("trace-1"));
    let request = json!({
        "model": "claude-opus-4-8-thinking",
        "max_tokens": 512,
        "system": "You are terse.",
        "thinking": {"type": "enabled", "budget_tokens": 1024},
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "What is in this picture?"},
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "aGVsbG8="}}
            ]},
            {"role": "assistant", "content": "A cat."},
            {"role": "user", "content": "Are you sure?"}
        ],
        "tools": [{"type": "custom", "name": "lookup", "input_schema": {"type": "object"}}]
    });
    let channel = ClaudeWeb::new();
    let response = channel
        .stream_generate_content(context(
            provider(&config, Some("https://claude.example/")),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            headers,
            request,
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers["content-type"], "text/event-stream");
    assert_eq!(
        client.sent().len(),
        4,
        "the conversation is deleted only once the stream is drained"
    );
    let text = drain(response).await;
    let sent = client.sent();
    assert_eq!(sent.len(), 5);

    let (method, url, headers, body) = &sent[0];
    assert_eq!(method, Method::POST);
    assert_eq!(url, "https://claude.example/api/org-1/upload");
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("multipart/form-data; boundary=")
    );
    let multipart = String::from_utf8_lossy(body);
    assert!(multipart.contains("filename=\"image.png\""), "{multipart}");
    assert!(multipart.contains("Content-Type: image/png\r\n\r\nhello"));
    assert_eq!(
        headers["cookie"],
        "sessionKey=sk-ant-sid01-example; anthropic-device-id=dev-1"
    );
    assert_eq!(headers["anthropic-device-id"], "dev-1");
    assert_eq!(headers["anthropic-client-platform"], "web_claude_ai");
    assert_eq!(headers["origin"], "https://claude.example");
    assert!(
        headers.get("x-api-key").is_none(),
        "source auth never leaves"
    );
    assert_eq!(
        headers["x-client-trace"], "trace-1",
        "other client headers pass"
    );

    let (method, url, _, body) = &sent[1];
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        "https://claude.example/api/organizations/org-1/chat_conversations"
    );
    let create: Value = serde_json::from_slice(body).unwrap();
    let conversation = create["uuid"].as_str().unwrap().to_owned();
    assert_eq!(conversation.len(), 36);
    assert_eq!(create["is_temporary"], true);
    assert_eq!(create["name"], "");

    let (method, url, headers, body) = &sent[2];
    assert_eq!(method, Method::PUT);
    assert_eq!(
        url,
        &format!(
            "https://claude.example/api/organizations/org-1/chat_conversations/{conversation}"
        )
    );
    assert_eq!(
        headers["referer"],
        format!("https://claude.example/chat/{conversation}")
    );
    let settings: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(
        settings["settings"]["paprika_mode"], "extended",
        "pro + thinking"
    );

    let (method, url, headers, body) = &sent[3];
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        &format!(
            "https://claude.example/api/organizations/org-1/chat_conversations/{conversation}/completion"
        )
    );
    assert_eq!(headers["accept"], "text/event-stream");
    assert_eq!(headers["content-type"], "application/json");
    let completion: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(
        completion["model"], "claude-opus-4-8",
        "the -thinking suffix is stripped"
    );
    assert_eq!(completion["thinking_mode"], "auto");
    assert_eq!(completion["max_tokens_to_sample"], 512);
    assert_eq!(completion["timezone"], "Europe/Berlin");
    assert_eq!(completion["rendering_mode"], "messages");
    assert_eq!(completion["files"], json!(["file-1"]));
    assert_eq!(completion["tools"][0]["name"], "lookup");
    assert!(completion["tools"][0].get("type").is_none());
    let prompt = completion["prompt"].as_str().unwrap();
    assert_eq!(
        prompt,
        "be brief\n\nYou are terse.\n\nHuman: What is in this picture?\n(image attached)\n\nAssistant: A cat.\n\nHuman: Are you sure?"
    );
    assert!(completion["turn_message_uuids"]["human_message_uuid"].is_string());

    let (method, url, _, _) = &sent[4];
    assert_eq!(method, Method::DELETE);
    assert_eq!(
        url,
        &format!(
            "https://claude.example/api/organizations/org-1/chat_conversations/{conversation}"
        )
    );

    let events = events(&text);
    let kinds: Vec<&str> = events.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop"
        ]
    );
    assert!(text.starts_with("event: message_start\ndata: "));
    assert_eq!(events[0]["message"]["id"], "msg-up");
    assert_eq!(events[0]["message"]["model"], "claude-opus-4-8");
    assert!(
        events[0]["message"]["usage"]["input_tokens"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(events[4]["usage"]["output_tokens"], 3, "ceil(11 chars / 4)");
    assert!(state.keys().is_empty(), "no tool boundary, no continuation");
    assert!(channel.registry().is_empty());
}

const TOOL_STREAM: &str = "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-up\",\"content\":[]}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu-1\",\"name\":\"weather\",\"input\":{}}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\\\"Oslo\\\"}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu-1\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Sunny\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null}}\n\ndata: {\"type\":\"message_stop\"}\n\n";

#[tokio::test]
async fn tool_use_parks_the_turn_and_tool_result_resumes_it() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TOOL_STREAM),
    ]);
    let state = MemoryState::new();
    let channel = ClaudeWeb::new();
    let request = json!({
        "model": "claude-sonnet-4-6",
        "messages": [{"role": "user", "content": "weather in Oslo?"}],
        "tools": [{"name": "weather", "input_schema": {"type": "object"}}]
    });
    let response = channel
        .stream_generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            request,
        ))
        .await
        .unwrap();
    let text = drain(response).await;
    let first = events(&text);
    let kinds: Vec<&str> = first.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop"
        ]
    );
    assert_eq!(first[4]["delta"]["stop_reason"], "tool_use");
    assert!(
        !text.contains("Sunny"),
        "the answer waits for the tool result"
    );
    let sent = client.sent();
    assert_eq!(sent.len(), 3, "no DELETE while a continuation is pending");
    assert_eq!(
        sent[0].1,
        "https://claude.ai/api/organizations/org-1/chat_conversations"
    );
    let conversation = serde_json::from_slice::<Value>(&sent[0].3).unwrap()["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    let key = continuation_key("toolu-1");
    assert_eq!(state.keys(), vec![key.clone()]);
    let entry = state.get(&key).await.unwrap().unwrap();
    let session: SessionState = serde_json::from_slice(&entry.payload).unwrap();
    assert_eq!(session.conversation, conversation);
    assert_eq!(session.model, "claude-sonnet-4-6");
    assert_eq!(session.message_id, "msg-up");
    let input_before = first[0]["message"]["usage"]["input_tokens"]
        .as_u64()
        .unwrap();
    assert_eq!(
        session.input_tokens,
        input_before + 4,
        "the tool call's own output joins the next message's input"
    );
    let expires_in = entry
        .expires_at
        .unwrap()
        .duration_since(SystemTime::now())
        .unwrap();
    assert!(expires_in > Duration::from_secs(9 * 60) && expires_in <= Duration::from_secs(10 * 60));
    assert_eq!(channel.registry().len(), 1);

    client.push(reply(StatusCode::OK, json!({})));
    client.push(reply(StatusCode::NO_CONTENT, json!({})));
    let follow_up = json!({
        "model": "claude-sonnet-4-6",
        "messages": [
            {"role": "user", "content": "weather in Oslo?"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu-1", "name": "weather", "input": {"city": "Oslo"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu-1", "content": "sunny, 21C"}]}
        ]
    });
    let response = channel
        .stream_generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            follow_up,
        ))
        .await
        .unwrap();
    let text = drain(response).await;
    let second = events(&text);
    let kinds: Vec<&str> = second.iter().map(|e| e["type"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        [
            "message_start",
            "content_block_start",
            "content_block_delta",
            "content_block_stop",
            "message_delta",
            "message_stop"
        ],
        "the echoed tool_result block is hidden"
    );
    assert_eq!(second[0]["message"]["id"], "msg-up");
    assert!(
        second[0]["message"]["usage"]["input_tokens"]
            .as_u64()
            .unwrap()
            > session.input_tokens
    );
    assert_eq!(second[2]["delta"]["text"], "Sunny");
    assert_eq!(second[4]["delta"]["stop_reason"], "end_turn");
    let sent = client.sent();
    assert_eq!(sent.len(), 5);
    let (method, url, _, body) = &sent[3];
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        &format!(
            "https://claude.ai/api/organizations/org-1/chat_conversations/{conversation}/tool_result"
        )
    );
    let posted: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(posted["tool_use_id"], "toolu-1");
    assert_eq!(
        posted["content"],
        json!([{"type": "text", "text": "sunny, 21C"}])
    );
    assert_eq!(sent[4].0, Method::DELETE);
    assert!(sent[4].1.ends_with(&conversation));
    assert!(state.keys().is_empty(), "the continuation is released");
    assert!(channel.registry().is_empty());

    // A second submission of the same tool result finds nothing to resume.
    let stale = json!({
        "model": "claude-sonnet-4-6",
        "messages": [{"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu-1", "content": "again"}]}]
    });
    let error = channel
        .stream_generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            stale,
        ))
        .await
        .expect_err("expired continuation");
    assert!(
        matches!(&error, ChannelError::Transport(e) if e.kind() == CapabilityErrorKind::Expired),
        "{error}"
    );
    let mixed = json!({
        "model": "claude-sonnet-4-6",
        "messages": [{"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "toolu-1", "content": "a"},
            {"type": "tool_result", "tool_use_id": "toolu-2", "content": "b"}
        ]}]
    });
    let error = channel
        .stream_generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            mixed,
        ))
        .await
        .expect_err("mixed continuations");
    assert!(
        matches!(&error, ChannelError::Transport(e) if e.kind() == CapabilityErrorKind::Invalid),
        "{error}"
    );
}

#[tokio::test]
async fn non_streaming_generate_collects_one_message() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TOOL_STREAM),
    ]);
    let state = MemoryState::new();
    let channel = ClaudeWeb::new();
    let response = channel
        .generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
        ))
        .await
        .unwrap();
    assert_eq!(response.headers["content-type"], "application/json");
    let message: Value = serde_json::from_str(&drain(response).await).unwrap();
    assert_eq!(message["type"], "message");
    assert_eq!(message["role"], "assistant");
    assert_eq!(message["id"], "msg-up");
    assert_eq!(message["stop_reason"], "tool_use");
    assert_eq!(message["content"][0]["type"], "tool_use");
    assert_eq!(message["content"][0]["id"], "toolu-1");
    assert_eq!(message["content"][0]["input"], json!({"city": "Oslo"}));
    assert_eq!(message["content"].as_array().unwrap().len(), 1);
    assert_eq!(message["usage"]["output_tokens"], 4);
    assert_eq!(
        client.sent().len(),
        3,
        "parked at the tool boundary, not deleted"
    );

    // A plain text answer through the same path, then the conversation goes.
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TEXT_STREAM),
        reply(StatusCode::NO_CONTENT, json!({})),
    ]);
    let response = channel
        .generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
        ))
        .await
        .unwrap();
    let message: Value = serde_json::from_str(&drain(response).await).unwrap();
    assert_eq!(
        message["content"],
        json!([{"type": "text", "text": "Hello there"}])
    );
    assert_eq!(message["stop_reason"], "end_turn");
    assert_eq!(client.sent()[3].0, Method::DELETE);

    // A failed completion deletes the conversation and surfaces the reply.
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        reply(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": "rate_limited"}),
        ),
        reply(StatusCode::NO_CONTENT, json!({})),
    ]);
    let error = channel
        .generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
        ))
        .await
        .err()
        .unwrap();
    assert!(matches!(
        error,
        ChannelError::UpstreamResponse { status, .. } if status == StatusCode::TOO_MANY_REQUESTS
    ));
    assert_eq!(client.sent()[3].0, Method::DELETE);
}

const BOOTSTRAP: &str = r#"{"account":{"email_address":"user@example.com","memberships":[{"organization":{"uuid":"org-api","capabilities":["api"]}},{"organization":{"uuid":"org-chat","capabilities":["chat","claude_pro"],"rate_limit_tier":"default_claude_pro","claude_ai_bootstrap_models_config":{"claude-z":{"displayName":"Claude Z"},"nested":[{"model_id":"claude-a","label":"Claude A"},"{\"model\":\"claude-z\"}",{"model":"not-claude"}]}}}]}}"#;

#[tokio::test]
async fn cookie_login_bootstraps_the_account() {
    let config = json!({});
    let client = ScriptClient::new(vec![WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(BOOTSTRAP.as_bytes())),
    }]);
    let channel = ClaudeWeb::new();
    let acquired = channel
        .cookie_login()
        .unwrap()
        .exchange_cookie(
            LoginContext {
                provider: provider(&config, Some("https://claude.example")),
                client: &*client,
            },
            "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example",
        )
        .await
        .unwrap();
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::GET);
    assert_eq!(sent[0].1, "https://claude.example/api/bootstrap");
    assert_eq!(sent[0].2["accept"], "application/json");
    assert_eq!(sent[0].2["referer"], "https://claude.example/new");
    let cookie = sent[0].2["cookie"].to_str().unwrap().to_owned();
    assert!(
        cookie.starts_with(
            "cf_clearance=clear; sessionKey=sk-ant-sid01-example; anthropic-device-id="
        )
    );
    assert_eq!(
        acquired.secret["cookie"],
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example"
    );
    assert_eq!(acquired.secret["organization_uuid"], "org-chat");
    assert_eq!(acquired.secret["device_id"].as_str().unwrap().len(), 36);
    assert_eq!(
        acquired.secret["capabilities"],
        json!(["chat", "claude_pro"])
    );
    assert_eq!(acquired.metadata["organization_uuid"], "org-chat");
    assert_eq!(acquired.metadata["user_email"], "user@example.com");
    assert_eq!(acquired.metadata["pro"], true);
    assert_eq!(acquired.metadata["rate_limit_tier"], "default_claude_pro");
    assert_eq!(
        acquired.metadata["models"],
        json!([{"id": "claude-a", "display_name": "Claude A"}, {"id": "claude-z", "display_name": "Claude Z"}])
    );
    let validated = acquired.metadata["validated_at_ms"].as_i64().unwrap();
    assert_eq!(
        acquired.expires_at_ms,
        Some(validated + 12 * 60 * 60 * 1000)
    );

    let listed = channel
        .list_models(OperationContext {
            provider: provider(&config, None),
            credential: credential(&acquired.secret, &acquired.metadata),
            dialect: Dialect::Claude,
            request: WireRequest {
                method: Method::GET,
                path: "/v1/models".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            client: client.clone(),
            state: MemoryState::new(),
            instance_id: Arc::from("local"),
            endpoint_override: None,
        })
        .await
        .unwrap();
    let list: Value = serde_json::from_str(&drain(listed).await).unwrap();
    assert_eq!(list["data"][0]["id"], "claude-a");
    assert_eq!(list["data"][1]["display_name"], "Claude Z");
    assert_eq!(list["first_id"], "claude-a");
    assert_eq!(list["has_more"], false);
    assert_eq!(client.sent().len(), 1, "the model list is local");
    assert!(
        matches!(
            channel
                .cookie_login()
                .unwrap()
                .exchange_cookie(
                    LoginContext {
                        provider: provider(&config, None),
                        client: &*client
                    },
                    "not a cookie",
                )
                .await,
            Err(ChannelError::InvalidCredential)
        ),
        "a cookie without sessionKey is refused before any call"
    );
}

#[tokio::test]
async fn refresh_revalidates_and_rejects_dead_sessions() {
    let config = json!({});
    let secret = json!({"cookie": "sessionKey=sk-ant-sid01-old", "organization_uuid": "org-old", "device_id": "dev-1", "note": "kept"});
    let client = ScriptClient::new(vec![
        WireResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(BOOTSTRAP.as_bytes())),
        },
        reply(StatusCode::UNAUTHORIZED, json!({})),
        reply(StatusCode::OK, json!({"account": null})),
        reply(StatusCode::BAD_GATEWAY, json!({})),
    ]);
    let channel = ClaudeWeb::new();
    let refresher = channel.credential_refresh().unwrap();
    let context = || CredentialContext {
        provider: provider(&config, None),
        credential: credential(&secret, &Value::Null),
        client: &*client,
    };
    let update = refresher.refresh(context()).await.unwrap();
    assert_eq!(update.secret["cookie"], "sessionKey=sk-ant-sid01-old");
    assert_eq!(update.secret["organization_uuid"], "org-chat");
    assert_eq!(update.secret["device_id"], "dev-1");
    assert_eq!(update.secret["note"], "kept");
    let validated = update.secret["validated_at_ms"].as_i64().unwrap();
    assert_eq!(update.expires_at_ms, Some(validated + 12 * 60 * 60 * 1000));
    assert_eq!(
        client.sent()[0].2["cookie"],
        "sessionKey=sk-ant-sid01-old; anthropic-device-id=dev-1"
    );

    let error = refresher.refresh(context()).await.err().unwrap();
    assert!(
        matches!(error, ChannelError::RefreshRejected(_)),
        "401 is final: {error}"
    );
    let error = refresher.refresh(context()).await.err().unwrap();
    assert!(
        matches!(error, ChannelError::RefreshRejected(_)),
        "logged out is final: {error}"
    );
    let error = refresher.refresh(context()).await.err().unwrap();
    assert!(
        matches!(error, ChannelError::UpstreamResponse { status, .. } if status == StatusCode::BAD_GATEWAY),
        "a 5xx is transient"
    );
}

/// Family, surface and per-model weekly limits and the weekly breakdown are
/// observed without a declared dimension.
const OBSERVE_ONLY: &[&str] = &["claudeweb_seven_day_*", "claudeweb_weekly_*"];

#[tokio::test]
async fn usage_windows_and_scoped_limits_become_quota_entries() {
    let config = json!({});
    let secret = secret();
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({
            "five_hour": { "utilization": 3.0, "resets_at": "2026-07-12T16:29:59.581984+00:00" },
            "seven_day": { "utilization": 61.0, "resets_at": "2026-07-17T21:59:59+00:00" },
            "seven_day_opus": { "utilization": 12.0, "resets_at": "2026-07-17T21:59:59+00:00" },
            "seven_day_sonnet": null,
            "limits": [
                { "kind": "weekly_scoped", "percent": 12.0, "resets_at": "2026-07-17T21:59:59Z",
                  "scope": { "model": { "id": "claude-opus-5", "display_name": "Opus" } } },
                { "kind": "weekly_scoped", "percent": 40.0, "resets_at": "2026-07-17T21:59:59Z",
                  "scope": { "surface": "Claude Code" } },
                { "kind": "weekly_scoped", "percent": 5.0, "resets_at": "2026-07-17T21:59:59Z",
                  "scope": { "model": { "id": "claude-haiku-5-20260101" } } },
                { "kind": "weekly_all", "percent": 61.0, "resets_at": "2026-07-17T21:59:59Z" }
            ]
        }),
    )]);
    let channel = ClaudeWeb::new();
    let snapshot = channel
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            client: &*client,
        })
        .await
        .unwrap();
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::GET);
    assert_eq!(sent[0].1, "https://claude.ai/api/organizations/org-1/usage");
    assert!(
        sent[0].2["cookie"]
            .to_str()
            .unwrap()
            .contains("sessionKey=sk-ant-sid01-example")
    );
    let ids: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|e| e.source_id.as_str())
        .collect();
    // The Opus limit duplicates seven_day_opus and is dropped in its favour.
    assert_eq!(
        ids,
        [
            "claudeweb_five_hour",
            "claudeweb_seven_day",
            "claudeweb_seven_day_opus",
            "claudeweb_weekly_surface_claude_code",
            "claudeweb_weekly_model_claude_haiku_5_20260101",
        ]
    );
    let QuotaValue::Window(five) = &snapshot.entries[0].value else {
        panic!("window");
    };
    assert_eq!(five.used_percent, Some(3.into()));
    assert_eq!(five.remaining, Some(97.into()));
    let end = five.period_end_ms.unwrap();
    assert_eq!(end, 1_783_873_799_000, "2026-07-12T16:29:59Z");
    assert_eq!(five.period_start_ms, Some(end - 5 * 60 * 60 * 1000));
    assert_eq!(snapshot.entries[0].model_scope, QuotaScope::All);
    assert_eq!(
        snapshot.entries[2].model_scope,
        QuotaScope::ModelPrefixes(vec!["claude-opus".into()])
    );
    assert_eq!(snapshot.entries[3].model_scope, QuotaScope::Unknown);
    let QuotaValue::Window(surface) = &snapshot.entries[3].value else {
        panic!("window");
    };
    assert_eq!(surface.used_percent, Some(40.into()));
    assert_eq!(
        surface.period_start_ms,
        surface
            .period_end_ms
            .map(|end| end - 7 * 24 * 60 * 60 * 1000)
    );
    assert_eq!(
        snapshot.entries[4].model_scope,
        QuotaScope::Models(vec!["claude-haiku-5-20260101".into()])
    );

    let dims = channel.quota_model().unwrap().dimensions(
        provider(&config, None),
        credential(&secret, &json!({"rate_limit_tier": "default_claude_pro"})),
    );
    assert_eq!(dims.len(), 2);
    assert_eq!(dims[0].id, "claudeweb_five_hour");
    assert_eq!(
        dims[0].label.as_deref(),
        Some("default_claude_pro 5h window")
    );
    assert_eq!(dims[1].id, "claudeweb_seven_day");
    let model = channel.quota_model();
    support::assert_quota_contract(model, &dims, &snapshot.entries, OBSERVE_ONLY);
    // The Claude Code usage capture has the same shape; its Fable window
    // stays observe-only here (claudeweb declares no family windows).
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        serde_json::from_str(include_str!("fixtures/quota/claudecode_usage.json")).unwrap(),
    )]);
    let captured = channel
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            client: &*client,
        })
        .await
        .unwrap();
    support::assert_quota_contract(model, &dims, &captured.entries, OBSERVE_ONLY);

    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"five_hour": null, "seven_day": null}),
    )]);
    let error = channel
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            client: &*client,
        })
        .await
        .err()
        .unwrap();
    assert!(matches!(error, ChannelError::InvalidResponse(_)), "{error}");
}

/// claude.ai reports no token counts; the translated Messages output carries
/// the channel's character estimates, and the reply settles with them marked
/// as estimates, on both paths.
#[tokio::test]
async fn a_turn_settles_with_estimated_counts() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let channel = ClaudeWeb::new();
    let request = json!({"model": "claude-sonnet-4-6",
        "messages": [{"role": "user", "content": "Say hello to everyone here"}]});
    for streamed in [true, false] {
        let client = ScriptClient::new(vec![
            reply(StatusCode::CREATED, json!({})),
            reply(StatusCode::OK, json!({})),
            sse(StatusCode::OK, TEXT_STREAM),
            reply(StatusCode::NO_CONTENT, json!({})),
        ]);
        let context = context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            MemoryState::new(),
            HeaderMap::new(),
            request.clone(),
        );
        let (operation, response) = if streamed {
            (
                Operation::StreamGenerateContent,
                channel.stream_generate_content(context).await.unwrap(),
            )
        } else {
            (
                Operation::GenerateContent,
                channel.generate_content(context).await.unwrap(),
            )
        };
        let headers = response.headers.clone();
        let body = drain(response).await;
        let usage = if streamed {
            support::settled_stream(
                &channel,
                operation,
                Dialect::Claude,
                &headers,
                body.as_bytes(),
            )
        } else {
            support::settled(
                &channel,
                operation,
                Dialect::Claude,
                &headers,
                body.as_bytes(),
            )
        }
        .expect("the estimates are read");
        assert!(usage.tokens.input_tokens.is_some_and(|tokens| tokens > 0));
        assert!(usage.tokens.output_tokens.is_some_and(|tokens| tokens > 0));
        assert_eq!(
            usage.completeness,
            gproxy_channel::channel::UsageCompleteness::Partial
        );
        assert_eq!(usage.dimensions["estimated"], "true");
    }
}

#[tokio::test]
async fn allowed_headers_and_channel_headers_are_enforced() {
    let config = json!({
        "allowed_headers": ["x-client-trace"],
        "headers": {"x-static": "yes"},
        "endpoints": {"claudeweb_completion": "https://mirror.example/{organization}/c/{conversation}/go"}
    });
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TEXT_STREAM),
        reply(StatusCode::NO_CONTENT, json!({})),
    ]);
    let mut headers = HeaderMap::new();
    headers.insert("x-client-trace", HeaderValue::from_static("trace-1"));
    headers.insert("x-other", HeaderValue::from_static("dropped"));
    headers.insert("user-agent", HeaderValue::from_static("anthropic-sdk"));
    headers.insert("origin", HeaderValue::from_static("https://evil.example"));
    headers.insert("anthropic-device-id", HeaderValue::from_static("spoof"));
    let channel = ClaudeWeb::new();
    let response = channel
        .stream_generate_content(context(
            provider(&config, None),
            credential(&secret, &metadata),
            client.clone(),
            MemoryState::new(),
            headers,
            json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
        ))
        .await
        .unwrap();
    drain(response).await;
    let sent = client.sent();
    let (_, url, headers, _) = &sent[2];
    assert!(url.starts_with("https://mirror.example/org-1/c/"));
    assert!(url.ends_with("/go"));
    assert_eq!(headers["x-client-trace"], "trace-1");
    assert_eq!(headers["x-static"], "yes");
    assert!(headers.get("x-other").is_none(), "not on the allow-list");
    assert!(
        headers.get("user-agent").is_none(),
        "the emulation profile owns it"
    );
    assert_eq!(headers["origin"], "https://claude.ai");
    assert_eq!(headers["anthropic-device-id"], "dev-1");
    assert_eq!(
        headers["cookie"],
        "sessionKey=sk-ant-sid01-example; anthropic-device-id=dev-1"
    );
    assert_eq!(
        sent[1].2["x-static"], "yes",
        "static headers reach every step"
    );

    let bad = json!({"allowed_headers": "nope"});
    let error = channel
        .stream_generate_content(context(
            provider(&bad, None),
            credential(&secret, &metadata),
            client.clone(),
            MemoryState::new(),
            HeaderMap::new(),
            json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
        ))
        .await
        .err()
        .unwrap();
    assert!(matches!(error, ChannelError::InvalidConfig(_)));
    assert_eq!(
        channel.native_dialects(provider(&config, None), Operation::StreamGenerateContent),
        vec![Dialect::Claude]
    );
    assert!(
        channel
            .native_dialects(provider(&config, None), Operation::CreateEmbedding)
            .is_empty()
    );
}

#[tokio::test]
async fn a_continuation_held_by_another_instance_is_named_not_expired() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TOOL_STREAM),
    ]);
    let state = MemoryState::new();
    let channel = ClaudeWeb::new();
    let request = json!({
        "model": "claude-sonnet-4-6",
        "messages": [{"role": "user", "content": "weather in Oslo?"}],
        "tools": [{"name": "weather", "input_schema": {"type": "object"}}]
    });
    let mut first = context(
        provider(&config, None),
        credential(&secret, &metadata),
        client.clone(),
        state.clone(),
        HeaderMap::new(),
        request,
    );
    first.instance_id = Arc::from("instance-a");
    let response = channel.stream_generate_content(first).await.unwrap();
    drain(response).await;
    assert_eq!(state.keys(), vec!["tool:toolu-1".to_owned()]);
    let sent_before = client.sent().len();

    // The tool result reaches another process: it must not touch the
    // conversation or the record, and it says who holds the continuation.
    let mut second = context(
        provider(&config, None),
        credential(&secret, &metadata),
        client.clone(),
        state.clone(),
        HeaderMap::new(),
        json!({
            "model": "claude-sonnet-4-6",
            "messages": [{"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu-1", "content": "sunny"}]}]
        }),
    );
    second.instance_id = Arc::from("instance-b");
    let error = channel
        .stream_generate_content(second)
        .await
        .expect_err("held elsewhere");
    assert!(
        matches!(&error, ChannelError::ContinuationElsewhere { instance_id } if instance_id == "instance-a"),
        "{error}"
    );
    assert_eq!(
        client.sent().len(),
        sent_before,
        "no upstream call was made"
    );
    assert_eq!(
        state.keys(),
        vec!["tool:toolu-1".to_owned()],
        "the record stays"
    );
    assert_eq!(channel.registry().len(), 1, "the connection stays parked");
}

// ----------------------------------------------------------- magic cache

const MAGIC_AUTO: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_5M: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_1H: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";
const MAGIC_PREFIX: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_";

#[tokio::test]
async fn magic_cache_strings_are_stripped_from_the_prompt() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::CREATED, json!({"uuid": "ignored"})),
        reply(StatusCode::OK, json!({})),
        sse(StatusCode::OK, TEXT_STREAM),
        reply(StatusCode::NO_CONTENT, json!({})),
    ]);
    let state = MemoryState::new();
    let request = json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 64,
        "system": [{"type": "text", "text": format!("You are terse.{MAGIC_1H}"), "cache_control": {"type": "ephemeral"}}],
        "messages": [
            {"role": "user", "content": format!("{MAGIC_AUTO}hello {MAGIC_5M}")}
        ]
    });
    let response = ClaudeWeb::new()
        .stream_generate_content(context(
            provider(&config, Some("https://claude.example/")),
            credential(&secret, &metadata),
            client.clone(),
            state.clone(),
            HeaderMap::new(),
            request,
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    drain(response).await;
    let sent = client.sent();
    let completion: Value = serde_json::from_slice(&sent[2].3).unwrap();
    assert_eq!(
        completion["prompt"].as_str().unwrap(),
        "You are terse.\n\nHuman: hello"
    );
    assert!(
        !String::from_utf8_lossy(&sent[2].3).contains(MAGIC_PREFIX),
        "no token reaches claude.ai"
    );
}
