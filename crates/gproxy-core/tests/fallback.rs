#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{ClaudeFallback, PrepareContext, ProviderView},
    channels::azure::Azure,
};
use gproxy_core::{ProviderData, RequestContext};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};
use support::{full, harness};

#[derive(Default)]
struct Client {
    replies: Mutex<VecDeque<WireResponse<HttpBody>>>,
    requests: Mutex<Vec<(String, Value)>>,
}
impl OutboundClient for Client {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let (head, body) = request.into_parts();
            let HttpBody::Bytes(body) = body else {
                panic!("buffered request")
            };
            self.requests
                .lock()
                .unwrap()
                .push((head.uri.to_string(), serde_json::from_slice(&body).unwrap()));
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply"))
        })
    }
}
struct ClaudeOnly;
impl BaseChannel for ClaudeOnly {
    fn id(&self) -> &'static str {
        "claude-only"
    }
    fn native_dialects(&self, _: ProviderView<'_>, op: Operation) -> Vec<Dialect> {
        if matches!(
            op,
            Operation::GenerateContent | Operation::StreamGenerateContent
        ) {
            vec![Dialect::Claude]
        } else {
            vec![]
        }
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        Azure.prepare(ctx)
    }
    fn claude_fallback(&self) -> Option<ClaudeFallback> {
        Azure.claude_fallback()
    }
}
fn context(
    h: &support::Harness,
    client: Arc<Client>,
    streaming: bool,
    dialect: Dialect,
    config: Value,
) -> Arc<RequestContext> {
    context_for_channel(h, client, streaming, dialect, config, Arc::new(ClaudeOnly))
}
fn context_for_channel(
    h: &support::Harness,
    client: Arc<Client>,
    streaming: bool,
    dialect: Dialect,
    config: Value,
    channel: Arc<dyn BaseChannel>,
) -> Arc<RequestContext> {
    let op = OperationKey {
        operation: if streaming {
            Operation::StreamGenerateContent
        } else {
            Operation::GenerateContent
        },
        dialect,
    };
    let mut ctx = (*h.context_for("p", op, "fallback", 1, None)).clone();
    let old = &ctx.target.provider;
    let mut entity = (*old.entity).clone();
    entity.config = config;
    ctx.target.provider = Arc::new(ProviderData {
        entity: Arc::new(entity),
        channel,
        credential_ids: old.credential_ids.clone(),
        models: old.models.clone(),
        operation_rules: vec![],
        operation_urls: Default::default(),
        rewrite_rule_sets: vec![],
        credential_strategy: old.credential_strategy,
        session_affinity: false,
    });
    ctx.target.upstream_model = Some("claude-fable-5".into());
    ctx.target.credentials = ctx
        .target
        .credentials
        .iter()
        .map(|old| {
            Arc::new(gproxy_core::CredentialData {
                id: old.id.clone(),
                provider_id: old.provider_id.clone(),
                label: None,
                auth_kind: old.auth_kind.clone(),
                enabled: true,
                metadata: old.metadata.clone(),
                client: client.clone(),
                websocket_client: client.clone(),
                state: old.state.clone(),
                quota: old.quota.clone(),
                limits: old.limits.clone(),
            })
        })
        .collect();
    Arc::new(ctx)
}
fn wire(stream: bool) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(
            json!({
                "model":"claude-fable-5","max_tokens":128,"stream":stream,
                "messages":[{"role":"user","content":"hi"}]
            })
            .to_string(),
        )),
    }
}
fn message(model: &str, stop: &str, text: &str) -> Value {
    json!({"id":"msg_1","type":"message","role":"assistant","model":model,
        "content": if text.is_empty() { json!([]) } else { json!([{"type":"text","text":text}]) },
        "stop_reason":stop,"stop_sequence":null,"usage":{"input_tokens":20,"output_tokens":if text.is_empty(){0}else{2}}})
}
fn reply(body: Value) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}
fn event(value: Value) -> Bytes {
    Bytes::from(format!(
        "event: {}\ndata: {}\n\n",
        value["type"].as_str().unwrap(),
        value
    ))
}
fn events(model: &str, stop: &str, text: &str, credit: bool) -> Vec<Bytes> {
    let mut result = vec![event(json!({"type":"message_start","message":{
        "id":"msg_1","type":"message","role":"assistant","model":model,"content":[],
        "stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":20,"output_tokens":0}
    }}))];
    if !text.is_empty() {
        result.push(event(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})));
        result.push(event(json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})));
        result.push(event(json!({"type":"content_block_stop","index":0})));
    }
    let mut delta = json!({"type":"message_delta","delta":{"stop_reason":stop,"stop_sequence":null},
        "usage":{"output_tokens":if text.is_empty(){0}else{2}}});
    if credit {
        delta["delta"]["stop_details"] = json!({"type":"refusal","fallback_credit_token":"credit", "fallback_has_prefill_claim":true});
    }
    result.push(event(delta));
    result.push(event(json!({"type":"message_stop"})));
    result
}
fn sse(chunks: Vec<Bytes>) -> WireResponse<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "text/event-stream".parse().unwrap());
    WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Stream(Box::pin(futures_util::stream::iter(
            chunks.into_iter().map(Ok),
        ))),
    }
}
fn config() -> Value {
    json!({"fallback_mode":"models","fallback_models":["claude-opus-4-8","claude-opus-4-8","claude-sonnet-4-6"]})
}

#[tokio::test]
async fn buffered_refusal_retries_and_accounts_each_actual_model() {
    let h = harness(full(), "round_robin").await;
    let client = Arc::new(Client::default());
    client.replies.lock().unwrap().extend([
        reply(message("claude-fable-5", "refusal", "")),
        reply(message("claude-opus-4-8", "end_turn", "ok")),
    ]);
    let exec = h
        .core
        .generate_content(
            context(&h, client.clone(), false, Dialect::Claude, config()),
            wire(false),
        )
        .await
        .unwrap();
    let (response, completion) = exec.into_parts();
    let response: Value = serde_json::from_str(&support::read(response.body).await).unwrap();
    assert_eq!(response["model"], "claude-opus-4-8");
    {
        let sent = client.requests.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1["model"], "claude-opus-4-8");
        assert!(sent.iter().all(|(_, body)| body.get("fallbacks").is_none()));
    }
    let report = completion.await.unwrap();
    assert_eq!(report.exchanges.len(), 2);
    assert_eq!(
        report.exchanges[0].upstream_model.as_deref(),
        Some("claude-fable-5")
    );
    assert_eq!(report.exchanges[0].usage.attempts[0].billable, Some(false));
    assert_eq!(
        report.exchanges[1].upstream_model.as_deref(),
        Some("claude-opus-4-8")
    );
}

#[tokio::test]
async fn streaming_empty_refusal_is_replaced_without_duplicate_lifecycle() {
    let h = harness(full(), "round_robin").await;
    let client = Arc::new(Client::default());
    client.replies.lock().unwrap().extend([
        sse(events("claude-fable-5", "refusal", "", false)),
        sse(events("claude-opus-4-8", "end_turn", "ok", false)),
    ]);
    let exec = h
        .core
        .stream_generate_content(
            context(&h, client.clone(), true, Dialect::Claude, config()),
            wire(true),
        )
        .await
        .unwrap();
    let (response, completion) = exec.into_parts();
    let text = support::read(response.body).await;
    assert_eq!(text.matches("event: message_start").count(), 1);
    assert_eq!(text.matches("event: message_stop").count(), 1);
    assert!(!text.contains("refusal"), "{text}");
    assert!(text.contains("ok"));
    let report = completion.await.unwrap();
    assert_eq!(report.exchanges.len(), 2);
    assert_eq!(report.exchanges[0].usage.attempts[0].billable, Some(false));
}

#[tokio::test]
async fn live_output_is_not_held_and_refusal_without_credit_is_not_replayed() {
    let h = harness(full(), "round_robin").await;
    let client = Arc::new(Client::default());
    let mut chunks = events("claude-fable-5", "refusal", "live", false);
    let tail = chunks.split_off(3);
    let (release, gate) = tokio::sync::oneshot::channel::<()>();
    let mut response = sse(vec![]);
    response.body = HttpBody::Stream(Box::pin(
        futures_util::stream::iter(chunks.into_iter().map(Ok)).chain(futures_util::stream::once(
            async move {
                gate.await.unwrap();
                Ok(Bytes::from(
                    tail.into_iter()
                        .flat_map(|b| b.to_vec())
                        .collect::<Vec<_>>(),
                ))
            },
        )),
    ));
    client.replies.lock().unwrap().push_back(response);
    let exec = h
        .core
        .stream_generate_content(
            context(&h, client.clone(), true, Dialect::Claude, config()),
            wire(true),
        )
        .await
        .unwrap();
    let (response, completion) = exec.into_parts();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!()
    };
    let text = tokio::time::timeout(Duration::from_secs(1), async {
        let mut text = String::new();
        while !text.contains("live") {
            text.push_str(std::str::from_utf8(&stream.next().await.unwrap().unwrap()).unwrap());
        }
        text
    })
    .await
    .expect("content must arrive before upstream EOF");
    assert!(text.contains("live"));
    release.send(()).unwrap();
    let rest = support::read(HttpBody::Stream(stream)).await;
    assert!(rest.contains("refusal"));
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    let report = completion.await.unwrap();
    assert_eq!(report.exchanges[0].usage.attempts[0].billable, Some(true));
}

#[tokio::test]
async fn partial_refusal_with_credit_continues_one_stream() {
    let h = harness(full(), "round_robin").await;
    let client = Arc::new(Client::default());
    client.replies.lock().unwrap().extend([
        sse(events("claude-fable-5", "refusal", "first", true)),
        sse({
            let mut chunks = events("claude-opus-4-8", "end_turn", "second", false);
            chunks[0] = Bytes::from(
                String::from_utf8(chunks[0].to_vec())
                    .unwrap()
                    .replace("\"input_tokens\":20", "\"input_tokens\":30"),
            );
            chunks
        }),
    ]);
    let exec = h
        .core
        .stream_generate_content(
            context(&h, client.clone(), true, Dialect::Claude, config()),
            wire(true),
        )
        .await
        .unwrap();
    let (response, completion) = exec.into_parts();
    let text = support::read(response.body).await;
    assert_eq!(text.matches("event: message_start").count(), 1);
    assert_eq!(text.matches("event: message_stop").count(), 1);
    assert!(
        text.contains("first") && text.contains("second") && text.contains("fallback"),
        "{text}"
    );
    assert!(!text.contains("\"stop_reason\":\"refusal\""), "{text}");
    let final_delta: Value = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .find(|event| event["type"] == "message_delta")
        .unwrap();
    assert_eq!(final_delta["usage"]["input_tokens"], 30);
    {
        let sent = client.requests.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1["fallback_credit_token"], "credit");
        assert_eq!(sent[1].1["messages"][1]["role"], "assistant");
    }
    assert_eq!(completion.await.unwrap().exchanges.len(), 2);
}

#[tokio::test]
async fn converted_openai_request_uses_native_claude_fallback() {
    let h = harness(full(), "round_robin").await;
    let client = Arc::new(Client::default());
    client.replies.lock().unwrap().extend([
        reply(message("claude-fable-5", "refusal", "")),
        reply(message("claude-opus-4-8", "end_turn", "ok")),
    ]);
    let mut request = wire(false);
    request.path = "/v1/chat/completions".into();
    let exec = h
        .core
        .generate_content(
            context(&h, client.clone(), false, Dialect::OpenAiChat, config()),
            request,
        )
        .await
        .unwrap();
    let (response, completion) = exec.into_parts();
    let text = support::read(response.body).await;
    assert!(text.contains("ok"), "{text}");
    assert_eq!(client.requests.lock().unwrap().len(), 2);
    assert_eq!(completion.await.unwrap().exchanges.len(), 2);
}

#[tokio::test]
async fn disabled_fallback_and_http_errors_are_not_retried() {
    for (settings, status) in [
        (json!({}), StatusCode::OK),
        (config(), StatusCode::BAD_REQUEST),
    ] {
        let h = harness(full(), "round_robin").await;
        let client = Arc::new(Client::default());
        let mut response = reply(message("claude-fable-5", "refusal", ""));
        response.status = status;
        client.replies.lock().unwrap().push_back(response);
        let exec = h
            .core
            .generate_content(
                context(&h, client.clone(), false, Dialect::Claude, settings),
                wire(false),
            )
            .await
            .unwrap();
        let (response, completion) = exec.into_parts();
        assert_eq!(response.status, status);
        support::read(response.body).await;
        completion.await.unwrap();
        assert_eq!(client.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn vercel_retries_with_namespaced_models_but_does_not_redeem_credits() {
    for produced in [false, true] {
        let h = harness(full(), "round_robin").await;
        let client = Arc::new(Client::default());
        client.replies.lock().unwrap().extend([
            sse(events(
                "anthropic/claude-fable-5",
                "refusal",
                if produced { "partial" } else { "" },
                true,
            )),
            sse(events("anthropic/claude-opus-4-8", "end_turn", "ok", false)),
        ]);
        let mut ctx = (*context_for_channel(
            &h,
            client.clone(),
            true,
            Dialect::Claude,
            config(),
            Arc::new(gproxy_channel::channels::vercel::Vercel),
        ))
        .clone();
        ctx.target.upstream_model = Some("anthropic/claude-fable-5".into());
        let exec = h
            .core
            .stream_generate_content(Arc::new(ctx), wire(true))
            .await
            .unwrap();
        let (response, completion) = exec.into_parts();
        let text = support::read(response.body).await;
        {
            let sent = client.requests.lock().unwrap();
            assert_eq!(sent.len(), if produced { 1 } else { 2 });
            if !produced {
                assert_eq!(sent[1].1["model"], "anthropic/claude-opus-4-8");
                assert!(sent[1].1.get("fallbacks").is_none());
                assert!(sent[1].1.get("fallback_credit_token").is_none());
                assert!(text.contains("ok"));
            } else {
                assert!(text.contains("partial") && text.contains("refusal"));
            }
        }
        completion.await.unwrap();
    }
}
