//! AggregateStream: a non-stream client over a force-streamed upstream collapses
//! the buffered event-stream back into one object (the codex/kiro fix, exercised
//! here on the simple openai channel via an explicit force-stream routing rule).

use super::*;

/// `(generate_content, chat) → (stream_generate_content, chat)` forces the
/// upstream to stream even for a non-stream client; the buffered SSE is folded
/// back into one `chat.completion`.
#[tokio::test]
async fn non_stream_client_collapses_forced_stream() {
    let rule = json!([
        { "id": 1, "provider_id": 1, "operation": "generate_content", "kind": "open_ai_chat_completions",
          "implementation": "transform_to", "dest_operation": "stream_generate_content",
          "dest_kind": "open_ai_chat_completions", "sort_order": 0, "enabled": true }
    ]);
    let bundle = bundle_with("routing_rules", rule);

    // The upstream "streams" (SSE) regardless of the client's stream flag; the
    // fake returns it as a buffered body (non-stream transport).
    let sse = concat!(
        "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",",
        "\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"he\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",",
        "\"choices\":[{\"index\":0,\"delta\":{\"content\":\"llo\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let fake = Arc::new(FakeUpstream::new(Bytes::from(sse), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model": "gpt-test",
        "stream": false,
        "messages": [{ "role": "user", "content": "hi" }]
    });
    let ctx = RequestCtx {
        request_id: "agg".into(),
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

    let outcome = crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    // The upstream was asked to STREAM, even though the client sent stream:false.
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen[0].uri.contains("/v1/chat/completions"),
        "uri: {}",
        seen[0].uri
    );
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(up["stream"], true, "upstream forced to stream: {up}");
    drop(seen);

    // The client got a single collapsed object, not raw SSE.
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected a buffered Full body, got a stream")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(v["object"], "chat.completion", "collapsed object: {v}");
    assert_eq!(v["choices"][0]["message"]["content"], "hello");
}
