//! End-to-end request/response conversion and provider rule tests.

use super::*;

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
    assert_openai_chat_request(&seen[0], "gpt-test", false); // member model rewrite
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
    assert_openai_chat_request(&seen[0], "gpt-test", true);
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
    assert_openai_chat_request(&seen[0], "gemini-pro", true);
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
