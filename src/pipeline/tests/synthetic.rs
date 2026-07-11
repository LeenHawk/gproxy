//! SynthesizeStream: a streaming client can route to a non-streaming upstream
//! while the gateway returns protocol-correct streaming bytes.

use futures_util::StreamExt;

use super::*;

#[tokio::test]
async fn streaming_chat_client_uses_non_stream_upstream_and_gets_sse() {
    let rules = json!([
        { "id": 1, "provider_id": 1, "operation": "stream_generate_content", "kind": "open_ai_chat_completions",
          "implementation": "transform_to", "dest_operation": "generate_content",
          "dest_kind": "open_ai_chat_completions", "sort_order": 0, "enabled": true }
    ]);
    let bundle = bundle_with("routing_rules", rules);
    let response = json!({
        "id":"chat_1","object":"chat.completion","created":1,"model":"gpt-test",
        "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model":"gpt-test","stream":true,
        "messages":[{"role":"user","content":"hi"}]
    });
    let ctx = RequestCtx {
        request_id: "synthetic-chat".into(),
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
    assert_eq!(outcome.headers["content-type"], "text/event-stream");
    let ResponseBody::Stream(mut stream) = outcome.body else {
        panic!("expected synthetic stream")
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("chat.completion.chunk"), "{text}");
    assert!(text.contains(r#""content":"hello""#), "{text}");
    assert!(text.ends_with("data: [DONE]\n\n"), "{text}");

    let seen = fake.seen.lock().unwrap();
    let upstream: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(upstream["stream"], false, "{upstream}");
}

#[tokio::test]
async fn gemini_json_stream_is_a_valid_array() {
    let rules = json!([
        { "id": 1, "provider_id": 1, "operation": "stream_generate_content", "kind": "gemini_generate_content",
          "implementation": "transform_to", "dest_operation": "generate_content",
          "dest_kind": "open_ai_chat_completions", "sort_order": 0, "enabled": true }
    ]);
    let bundle = bundle_with("routing_rules", rules);
    let response = json!({
        "id":"chat_1","object":"chat.completion","created":1,"model":"gpt-test",
        "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({"contents":[{"role":"user","parts":[{"text":"hi"}]}]});
    let ctx = RequestCtx {
        request_id: "synthetic-gemini".into(),
        method: Method::POST,
        path: "/v1beta/models/gpt-test:streamGenerateContent".into(),
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
    assert_eq!(outcome.headers["content-type"], "application/json");
    let ResponseBody::Stream(mut stream) = outcome.body else {
        panic!("expected synthetic stream")
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.unwrap());
    }
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value[0]["candidates"][0]["content"]["parts"][0]["text"],
        "hello"
    );

    let seen = fake.seen.lock().unwrap();
    assert_openai_chat_request(&seen[0], "gpt-test", false);
}
