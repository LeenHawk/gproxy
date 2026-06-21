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
        route_name: None,
        pending_micros: 0,
    }
}

/// Settlement is detached (spawned) — poll until the usage row lands.
async fn wait_usage(state: &AppState) -> Usage {
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
        "provider_models",
        json!([{
            "id": 1, "provider_id": 1, "model_id": "gpt-test", "display_name": null,
            "pricing_json": { "input": "3", "output": "15" },
            "variants_json": null, "enabled": true
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
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

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

/// BUNDLE + pricing on gpt-test + a user-scope quota row (M6 Task 4 tests).
fn quota_bundle() -> String {
    let mut v: Value =
        serde_json::from_str(&bundle_with("quotas", json!([{ "id": 1, "scope": "user", "scope_id": 1, "quota_total": "100.00", "cost_used": "0" }]))).unwrap();
    v["provider_models"] = json!([{
        "id": 1, "provider_id": 1, "model_id": "gpt-test", "display_name": null,
        "pricing_json": { "input": "3", "output": "15" },
        "variants_json": null, "enabled": true
    }]);
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
