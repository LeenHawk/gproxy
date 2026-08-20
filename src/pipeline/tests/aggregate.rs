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
    let fake = Arc::new(
        FakeUpstream::new(Bytes::from(sse), vec![])
            .with_response_content_type("text/plain; charset=utf-8"),
    );
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
        body_model: None,
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
    assert_eq!(outcome.headers["content-type"], "application/json");
}

/// Cross-kind force-stream routes must normalize the operation used by the
/// transform context while preserving the routed streaming operation.
#[tokio::test]
async fn non_stream_chat_collapses_forced_responses_stream() {
    let rule = json!([
        { "id": 1, "provider_id": 1, "operation": "generate_content", "kind": "open_ai_chat_completions",
          "implementation": "transform_to", "dest_operation": "stream_generate_content",
          "dest_kind": "open_ai_responses", "sort_order": 0, "enabled": true }
    ]);
    let bundle = bundle_with("routing_rules", rule);

    let sse = concat!(
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",",
        "\"object\":\"response\",\"created_at\":1,\"status\":\"completed\",\"model\":\"gpt-test\",",
        "\"output\":[{\"id\":\"msg_1\",\"type\":\"message\",\"status\":\"completed\",",
        "\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"hello\",",
        "\"annotations\":[]}]}]}}\n\n",
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
        request_id: "cross-kind-agg".into(),
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
        body_model: None,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx)
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    {
        let seen = fake.seen.lock().unwrap();
        assert!(seen[0].uri.contains("/responses"), "uri: {}", seen[0].uri);
        let upstream: Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(upstream["stream"], true);
        assert_eq!(upstream["input"][0]["content"], "hi");
    }

    let ResponseBody::Full(body) = outcome.body else {
        panic!("expected a buffered Full body")
    };
    let response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(response["object"], "chat.completion");
    assert_eq!(response["choices"][0]["message"]["content"], "hello");
    assert_eq!(outcome.headers["content-type"], "application/json");
}

/// `(create_image, open_ai) → (stream_generate_content, open_ai_responses)` is a
/// codex-shaped force-stream: the images request is reshaped into a streaming
/// Responses call carrying the `image_generation` tool, and the buffered
/// Responses SSE is collapsed back into an images response — the
/// `image_generation_call` base64 result becomes `data[0].b64_json`.
///
/// Regression: `plan_for` force-streamed only `generate_content`, so an image
/// route fell into a plain non-stream buffered transform. Against codex that
/// hangs (the backend generates the image asynchronously and never returns a
/// complete non-stream body), surfacing as a ~60s gateway timeout / 502.
#[tokio::test]
async fn non_stream_image_request_collapses_forced_responses_stream() {
    let rule = json!([
        { "id": 1, "provider_id": 1, "operation": "create_image", "kind": "open_ai",
          "implementation": "transform_to", "dest_operation": "stream_generate_content",
          "dest_kind": "open_ai_responses", "sort_order": 0, "enabled": true }
    ]);
    let mut bundle: Value = serde_json::from_str(&bundle_with("routing_rules", rule)).unwrap();
    bundle["price_rules"] = json!([{
        "id": 1, "provider_id": 1, "match_type": "exact", "model_match": "gpt-test",
        "input_price": "0", "output_price": "0", "cache_read_price": "0",
        "cache_creation_5m_price": "0", "cache_creation_30m_price": "0",
        "cache_creation_1h_price": "0", "image_output_price": "40", "enabled": true
    }]);
    let bundle = bundle.to_string();

    // The upstream "streams" a Responses event-stream whose final output item is
    // the generated image; the fake returns it as a buffered body.
    let sse = concat!(
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"created_at\":1,",
        "\"object\":\"response\",\"output\":[],\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":",
        "{\"type\":\"image_generation_call\",\"id\":\"ig_1\",\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":",
        "{\"type\":\"image_generation_call\",\"id\":\"ig_1\",\"result\":\"AAAA\",\"status\":\"completed\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"created_at\":1,",
        "\"object\":\"response\",\"output\":[],\"status\":\"completed\",",
        "\"usage\":{\"input_tokens\":25,\"output_tokens\":1040,\"total_tokens\":1065,",
        "\"output_tokens_details\":{\"reasoning_tokens\":0,\"image_tokens\":1000}}}}\n\n",
    );
    let fake = Arc::new(FakeUpstream::new(Bytes::from(sse), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({ "model": "gpt-test", "prompt": "a red cube" });
    let ctx = RequestCtx {
        request_id: "img-agg".into(),
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
    assert_eq!(outcome.status, StatusCode::OK);

    // Upstream was force-streamed to the Responses endpoint with the image tool.
    {
        let seen = fake.seen.lock().unwrap();
        assert!(seen[0].uri.contains("/responses"), "uri: {}", seen[0].uri);
        let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
        assert_eq!(up["stream"], true, "upstream forced to stream: {up}");
        assert_eq!(
            up["tools"][0]["type"], "image_generation",
            "image tool injected: {up}"
        );
    }

    // The client got a collapsed images response, not raw SSE.
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected a buffered Full body, got a stream")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    assert_eq!(
        v["data"][0]["b64_json"], "AAAA",
        "collapsed image base64 surfaced: {v}"
    );
    let usage = super::billing::wait_usage(&state).await;
    assert_eq!(usage.input_tokens, 25);
    assert_eq!(usage.output_tokens, 40);
    assert_eq!(usage.image_output_tokens, 1000);
    assert_eq!(usage.cost, "0.04".parse().unwrap());
}
