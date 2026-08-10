//! M6 §17 settlement integration: upstream usage on a normal stream end,
//! the counting ladder on client drop, and include_usage injection.

use super::*;
use crate::store::persistence::records::Usage;
fn openai_stream_ctx(request_id: &str, model: &str) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model": model, "stream": true,
        "messages": [{ "role": "user", "content": "hi there" }]
    });
    RequestCtx {
        request_id: request_id.into(),
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
        mode: RoutingMode::Aggregated,
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    }
}

/// Settlement is detached (spawned) — poll until the usage row lands.
pub(super) async fn wait_usage(state: &AppState) -> Usage {
    for _ in 0..200 {
        let rows = state.persistence.list_usages(10).await.expect("list");
        if let Some(row) = rows.into_iter().next() {
            return row;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("usage row never appeared");
}

#[tokio::test]
async fn normal_stream_settles_upstream_usage() {
    let chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":null}]}"#;
    let usage_chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[],"usage":{"prompt_tokens":1000,"completion_tokens":500}}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("{chunk}\n\n")),
            Bytes::from(format!("{usage_chunk}\n\ndata: [DONE]\n\n")),
        ],
    ));
    let bundle = bundle_with(
        "price_rules",
        json!([{
            "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
            "operation": null, "kind": null,
            "input_price": "3", "output_price": "15",
            "cache_read_price": "0", "cache_creation_5m_price": "0",
            "cache_creation_1h_price": "0", "image_output_price": "0",
            "priority": 0, "enabled": true
        }]),
    );
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let outcome = crate::pipeline::execute(&state, openai_stream_ctx("bill-1", "claude-test"))
        .await
        .expect("pipeline ok");
    let ResponseBody::Stream(s) = outcome.body else {
        panic!("expected Stream")
    };
    use futures_util::StreamExt;
    let relayed: Vec<Bytes> = s.map(|r| r.expect("chunk ok")).collect().await;
    assert!(!relayed.is_empty());

    let row = wait_usage(&state).await;
    assert_eq!(row.request_id, "bill-1");
    assert_eq!(row.usage_source, "upstream");
    assert_eq!(row.ended, "complete");
    assert_eq!(row.input_tokens, 1000);
    assert_eq!(row.output_tokens, 500);
    // 1000 × 3/M + 500 × 15/M
    assert_eq!(row.cost, "0.0105".parse().unwrap());
    assert_eq!(row.model.as_deref(), Some("gpt-test"));
}

#[tokio::test]
async fn transformed_buffered_settles_provider_usage_before_response_conversion() {
    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "total_tokens": 1500,
            "prompt_tokens_details": { "cached_tokens": 600 }
        }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-test", false))
        .await
        .expect("pipeline ok");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("expected Full")
    };
    let returned: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(returned["usage"]["input_tokens"], 400);
    assert_eq!(returned["usage"]["cache_read_input_tokens"], 600);

    let row = wait_usage(&state).await;
    assert_eq!(row.usage_source, "upstream");
    assert_eq!(row.input_tokens, 400);
    assert_eq!(row.cache_read_tokens, 600);
    assert_eq!(row.output_tokens, 500);
}

#[tokio::test]
async fn compact_content_settles_usage() {
    let compact_response = json!({
        "id": "resp-compact-1",
        "created_at": 0,
        "object": "response.compaction",
        "output": [],
        "usage": {
            "input_tokens": 1200,
            "output_tokens": 80,
            "total_tokens": 1280,
            "input_tokens_details": { "cached_tokens": 200 },
            "output_tokens_details": { "reasoning_tokens": 20 }
        }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&compact_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;
    let mut ctx = openai_stream_ctx("bill-compact", "claude-test");
    ctx.path = "/v1/responses/compact".into();
    ctx.body = Bytes::from(
        serde_json::to_vec(&json!({
            "model": "claude-test",
            "input": [{ "role": "user", "content": "compact this" }]
        }))
        .unwrap(),
    );

    crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");

    let row = wait_usage(&state).await;
    assert_eq!(row.request_id, "bill-compact");
    assert_eq!(row.operation, "compact_content");
    assert_eq!(row.usage_source, "upstream");
    assert_eq!(row.input_tokens, 1000);
    assert_eq!(row.cache_read_tokens, 200);
    assert_eq!(row.output_tokens, 80);
}

#[tokio::test]
async fn image_sse_streams_and_settles_completed_event_usage() {
    let partial = json!({
        "type": "image_generation.partial_image",
        "partial_image_index": 0,
        "b64_json": "AA"
    });
    let completed = json!({
        "type": "image_generation.completed",
        // Regression: the general SSE decoder caps frames at 1 MiB. Image
        // settlement must discard this payload from its private transcript
        // without touching the bytes relayed to the client.
        "b64_json": "A".repeat((1024 * 1024) + 1),
        "usage": {
            "prompt_tokens": 25,
            "completion_tokens": 1000,
            "total_tokens": 1025
        }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("data: {partial}\n\n")),
            Bytes::from(format!("data: {completed}\n\n")),
        ],
    ));
    let mut bundle: Value = serde_json::from_str(&bundle_with(
        "routing_rules",
        json!([{
            "id": 1, "provider_id": 1, "operation": "create_image", "kind": "open_ai",
            "implementation": "passthrough", "dest_operation": null, "dest_kind": null,
            "sort_order": 0, "enabled": true
        }]),
    ))
    .unwrap();
    bundle["providers"][0]["channel"] = json!("custom");
    bundle["price_rules"] = json!([{
        "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
        "input_price": "0", "output_price": "0", "cache_read_price": "0",
        "cache_creation_5m_price": "0", "cache_creation_30m_price": "0",
        "cache_creation_1h_price": "0", "image_output_price": "40", "enabled": true
    }]);
    let (state, _dir) =
        state_with_bundle(Arc::clone(&fake), &serde_json::to_string(&bundle).unwrap()).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({ "model": "gpt-test", "prompt": "a red cube", "stream": true });
    let ctx = RequestCtx {
        request_id: "bill-image-sse".into(),
        method: Method::POST,
        path: "/v1/images/generations".into(),
        query: None,
        headers,
        body: Bytes::from(serde_json::to_vec(&body).unwrap()),
        mode: RoutingMode::Scoped {
            provider: "oai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");
    let ResponseBody::Stream(stream) = outcome.body else {
        panic!("stream:true image request must relay SSE")
    };
    use futures_util::StreamExt;
    let relayed = stream
        .map(|chunk| chunk.expect("image stream chunk"))
        .collect::<Vec<_>>()
        .await;
    let relayed = String::from_utf8(relayed.concat()).unwrap();
    assert!(relayed.contains("image_generation.partial_image"));
    assert!(relayed.contains("image_generation.completed"));
    assert!(relayed.len() > 1024 * 1024);

    let row = wait_usage(&state).await;
    assert_eq!(row.input_tokens, 25);
    assert_eq!(row.output_tokens, 0);
    assert_eq!(row.image_output_tokens, 1000);
    assert_eq!(row.cost, "0.04".parse().unwrap());
}

#[tokio::test]
async fn rerank_total_tokens_settle_as_input_usage() {
    let response = json!({
        "id": "rerank-1",
        "model": "gpt-test",
        "results": [{ "index": 0, "relevance_score": 0.9 }],
        "usage": { "total_tokens": 1500, "search_units": 1 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&response).unwrap()),
        vec![],
    ));
    let mut bundle: Value = serde_json::from_str(BUNDLE).unwrap();
    bundle["providers"][0]["channel"] = json!("custom");
    bundle["providers"][0]["settings_json"]["endpoints"]["openai_rerank"] =
        json!("http://fake.local/v1/rerank");
    bundle["price_rules"] = json!([{
        "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
        "input_price": "2", "output_price": "0", "cache_read_price": "0",
        "cache_creation_5m_price": "0", "cache_creation_30m_price": "0",
        "cache_creation_1h_price": "0", "image_output_price": "0", "enabled": true
    }]);
    bundle["rate_limits"] = json!([{
        "id": 9, "scope": "user", "scope_id": 1, "route_pattern": "*",
        "rpm": null, "rpd": null, "total_tokens": 10000
    }]);
    let (state, _dir) =
        state_with_bundle(Arc::clone(&fake), &serde_json::to_string(&bundle).unwrap()).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let ctx = RequestCtx {
        request_id: "bill-rerank".into(),
        method: Method::POST,
        path: "/v1/rerank".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "model": "gpt-test",
                "query": "test",
                "documents": ["doc a", "doc b"]
            }))
            .unwrap(),
        ),
        mode: RoutingMode::Scoped {
            provider: "oai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    };

    crate::pipeline::execute(&state, ctx)
        .await
        .expect("rerank pipeline ok");
    let row = wait_usage(&state).await;
    assert_eq!(row.operation, "rerank");
    assert_eq!(row.input_tokens, 1500);
    assert_eq!(row.output_tokens, 0);
    assert_eq!(row.cost, "0.003".parse().unwrap());

    let now = crate::util::time::unix_now();
    let current_daily = format!("rlt:9:d{}", now / 86_400);
    let previous_daily = format!("rlt:9:d{}", (now / 86_400) - 1);
    let daily = state
        .cache
        .get(&current_daily)
        .await
        .or(state.cache.get(&previous_daily).await)
        .expect("rerank daily token counter");
    assert_eq!(daily, b"1500");

    let current_minute = format!("ctpm:1:m{}", now / 60);
    let previous_minute = format!("ctpm:1:m{}", (now / 60) - 1);
    let credential_tpm = state
        .cache
        .get(&current_minute)
        .await
        .or(state.cache.get(&previous_minute).await)
        .expect("rerank credential token counter");
    assert_eq!(credential_tpm, b"1500");
}

#[tokio::test]
async fn transformed_compact_settles_target_usage_and_cache_ttl() {
    let claude_response = json!({
        "id": "msg-compact-1", "type": "message", "role": "assistant", "model": "claude-test",
        "content": [{ "type": "text", "text": "summary" }],
        "stop_reason": "end_turn", "stop_sequence": null,
        "usage": {
            "input_tokens": 100, "output_tokens": 20,
            "cache_read_input_tokens": 60,
            "cache_creation_input_tokens": 30,
            "cache_creation": {
                "ephemeral_5m_input_tokens": 10,
                "ephemeral_1h_input_tokens": 20
            }
        }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&claude_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;
    let mut ctx = openai_stream_ctx("bill-compact-target", "claude-direct");
    ctx.path = "/v1/responses/compact".into();
    ctx.body = Bytes::from(
        serde_json::to_vec(&json!({
            "model": "claude-direct",
            "input": [{ "role": "user", "content": "compact this" }]
        }))
        .unwrap(),
    );

    let outcome = crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("expected Full")
    };
    let returned: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(returned["usage"]["input_tokens"], 190);
    assert_eq!(
        returned["usage"]["input_tokens_details"]["cached_tokens"],
        60
    );
    assert_eq!(
        returned["usage"]["input_tokens_details"]["cache_write_tokens"],
        30
    );

    let row = wait_usage(&state).await;
    assert_eq!(row.input_tokens, 100);
    assert_eq!(row.output_tokens, 20);
    assert_eq!(row.cache_read_tokens, 60);
    assert_eq!(row.cache_creation_5m_tokens, 10);
    assert_eq!(row.cache_creation_30m_tokens, 0);
    assert_eq!(row.cache_creation_1h_tokens, 20);
}

/// GPT-5.6+ cache writes (`prompt_tokens_details.cache_write_tokens`) must
/// survive both the settle path and the openai→claude response conversion.
#[tokio::test]
async fn openai_cache_write_settles_and_converts() {
    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
        "usage": {
            "prompt_tokens": 1000,
            "completion_tokens": 500,
            "total_tokens": 1500,
            "prompt_tokens_details": { "cached_tokens": 600, "cache_write_tokens": 200 }
        }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let bundle = bundle_with(
        "price_rules",
        json!([{
            "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
            "input_price": "3", "output_price": "15", "cache_read_price": "0",
            "cache_creation_5m_price": "0", "cache_creation_30m_price": "3.75",
            "cache_creation_1h_price": "0", "image_output_price": "0", "enabled": true
        }]),
    );
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("claude-test", false))
        .await
        .expect("pipeline ok");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("expected Full")
    };
    let returned: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(returned["usage"]["input_tokens"], 200);
    assert_eq!(returned["usage"]["cache_read_input_tokens"], 600);
    assert_eq!(returned["usage"]["cache_creation_input_tokens"], 200);

    let row = wait_usage(&state).await;
    assert_eq!(row.input_tokens, 200);
    assert_eq!(row.cache_read_tokens, 600);
    assert_eq!(row.cache_creation_30m_tokens, 200);
    // 200 input × $3/M + 500 output × $15/M + 200 cache write × $3.75/M.
    assert_eq!(row.cost, "0.00885".parse().unwrap());
}

#[tokio::test]
async fn transformed_stream_settles_provider_usage_before_response_conversion() {
    let chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":null}]}"#;
    let usage_chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[],"usage":{"prompt_tokens":1000,"completion_tokens":500,"total_tokens":1500,"prompt_tokens_details":{"cached_tokens":600}}}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("{chunk}\n\n")),
            Bytes::from(format!("{usage_chunk}\n\ndata: [DONE]\n\n")),
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
    let relayed: Vec<Bytes> = s.map(|r| r.expect("chunk ok")).collect().await;
    assert!(!relayed.is_empty());

    let row = wait_usage(&state).await;
    assert_eq!(row.usage_source, "upstream");
    assert_eq!(row.input_tokens, 400);
    assert_eq!(row.cache_read_tokens, 600);
    assert_eq!(row.output_tokens, 500);
}

#[tokio::test]
async fn client_drop_settles_estimated() {
    let chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"partial output text"},"finish_reason":null}]}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![
            Bytes::from(format!("{chunk}\n\n")),
            Bytes::from_static(b"data: [DONE]\n\n"),
        ],
    ));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, openai_stream_ctx("bill-2", "claude-test"))
        .await
        .expect("pipeline ok");
    let ResponseBody::Stream(mut s) = outcome.body else {
        panic!("expected Stream")
    };
    use futures_util::StreamExt;
    let first = s.next().await.expect("one chunk").expect("chunk ok");
    assert!(!first.is_empty());
    drop(s); // client gone — the Drop guard settles Interrupted

    let row = wait_usage(&state).await;
    assert_eq!(row.ended, "interrupted");
    assert!(
        row.usage_source == "estimated" || row.usage_source == "counted",
        "source: {}",
        row.usage_source
    );
    assert!(row.output_tokens > 0, "buffered text counted");
    assert!(row.input_tokens > 0, "request body counted");
}

#[tokio::test]
async fn include_usage_injected() {
    let chunk = r#"data: {"id":"c","object":"chat.completion.chunk","created":0,"model":"gpt-test","choices":[{"index":0,"delta":{"content":"hi"},"finish_reason":null}]}"#;
    let fake = Arc::new(FakeUpstream::new(
        Bytes::new(),
        vec![Bytes::from(format!("{chunk}\n\ndata: [DONE]\n\n"))],
    ));
    let bundle = bundle_with(
        "routing_rules",
        json!([
            {
                "id": 1, "provider_id": 1, "operation": "stream_generate_content",
                "kind": "open_ai_chat_completions", "implementation": "passthrough",
                "dest_operation": null, "dest_kind": null, "sort_order": 0, "enabled": true
            },
            {
                "id": 2, "provider_id": 1, "operation": "generate_content",
                "kind": "open_ai_chat_completions", "implementation": "passthrough",
                "dest_operation": null, "dest_kind": null, "sort_order": 1, "enabled": true
            },
            {
                "id": 3, "provider_id": 1, "operation": "stream_generate_content",
                "kind": "claude_messages", "implementation": "transform_to",
                "dest_operation": "stream_generate_content",
                "dest_kind": "open_ai_chat_completions", "sort_order": 2, "enabled": true
            },
            {
                "id": 4, "provider_id": 1, "operation": "generate_content",
                "kind": "claude_messages", "implementation": "transform_to",
                "dest_operation": "stream_generate_content",
                "dest_kind": "open_ai_chat_completions", "sort_order": 3, "enabled": true
            }
        ]),
    );
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    // transform path: claude inbound → openai-chat upstream stream
    crate::pipeline::execute(&state, claude_ctx("claude-test", true))
        .await
        .expect("transform path ok");
    // passthrough path: openai-chat inbound stream
    crate::pipeline::execute(&state, openai_stream_ctx("bill-3", "claude-test"))
        .await
        .expect("passthrough path ok");

    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for (i, s) in seen.iter().enumerate() {
        let v: Value = serde_json::from_slice(&s.body).unwrap();
        assert_eq!(
            v["stream_options"]["include_usage"], true,
            "attempt {i}: {v}"
        );
        assert_eq!(v["stream"], true, "attempt {i} still streams");
    }
}

/// BUNDLE + price rule on gpt-test + a user-scope quota row (M6 Task 4 tests).
fn quota_bundle() -> String {
    let mut v: Value =
        serde_json::from_str(&bundle_with("quotas", json!([{ "id": 1, "scope": "user", "scope_id": 1, "quota_total": "100.00", "cost_used": "0" }]))).unwrap();
    v["price_rules"] = json!([
        {
            "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
            "operation": null, "kind": null,
            "input_price": "3", "output_price": "15",
            "cache_read_price": "0", "cache_creation_5m_price": "0",
            "cache_creation_1h_price": "0", "image_output_price": "0",
            "priority": 0, "enabled": true
        }
    ]);
    serde_json::to_string(&v).unwrap()
}

#[tokio::test]
async fn quota_reconciles_after_settle() {
    use crate::store::persistence::records::Scope;

    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1000, "completion_tokens": 500, "total_tokens": 1500 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &quota_bundle()).await;

    crate::pipeline::execute(&state, claude_ctx("claude-test", false))
        .await
        .expect("pipeline ok");

    // settle persists actual cost into the quota row (read-modify-write)
    let mut quota = None;
    for _ in 0..200 {
        let q = state
            .persistence
            .get_quota(Scope::User, 1)
            .await
            .expect("get quota")
            .expect("quota row");
        if q.cost_used > rust_decimal::Decimal::ZERO {
            quota = Some(q);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let quota = quota.expect("cost_used never reconciled");
    let row = wait_usage(&state).await;
    assert_eq!(quota.cost_used, row.cost, "quota charged the settled cost");
    // 1000 × 3/M + 500 × 15/M
    assert_eq!(quota.cost_used, "0.0105".parse().unwrap());

    // pending was refunded by the exact pre-deducted amount
    let pending = state.cache.incr("qp:user:1", 0, None).await.unwrap();
    assert!(pending <= 1, "pending refunded, got {pending} micros");
}

#[tokio::test]
async fn failed_request_refunds_pending() {
    use crate::store::persistence::records::Scope;

    let mut upstream = FakeUpstream::new(Bytes::from_static(b"{\"error\":\"boom\"}"), vec![]);
    upstream.statuses = vec![StatusCode::INTERNAL_SERVER_ERROR]; // every attempt 500s
    let fake = Arc::new(upstream);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &quota_bundle()).await;

    let result = crate::pipeline::execute(&state, claude_ctx("claude-test", false)).await;
    assert!(result.is_err(), "all-500 upstream must error");

    // refund-on-error in execute: pending back to 0, nothing persisted
    let pending = state.cache.incr("qp:user:1", 0, None).await.unwrap();
    assert_eq!(pending, 0, "pending refunded on pipeline error");
    let q = state
        .persistence
        .get_quota(Scope::User, 1)
        .await
        .expect("get quota")
        .expect("quota row");
    assert_eq!(q.cost_used, rust_decimal::Decimal::ZERO);
}
