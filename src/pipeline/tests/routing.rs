//! Aggregated/scoped route resolution and candidate ownership tests.

use super::*;

#[tokio::test]
async fn aggregated_bare_provider_model_is_not_inferred() {
    let fake = Arc::new(FakeUpstream::new(Bytes::from("{}"), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let err = match crate::pipeline::execute(&state, claude_ctx("gpt-test", false)).await {
        Ok(_) => panic!("bare provider model should not resolve in aggregated mode"),
        Err(err) => err,
    };
    assert!(
        matches!(err, crate::pipeline::error::PipelineError::UnknownRoute(model) if model == "gpt-test")
    );
    assert!(fake.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn aggregated_provider_model_direct_addressing_works() {
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

    let outcome = crate::pipeline::execute(&state, claude_ctx("oai/gpt-test", false))
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    let seen = fake.seen.lock().unwrap();
    assert_openai_chat_request(&seen[0], "gpt-test", false);
}

#[tokio::test]
async fn aggregated_global_alias_then_provider_alias() {
    let bundle = bundle_with(
        "aliases",
        json!([
            { "id": 1, "provider": "*", "alias": "codex/(.+)", "target": "oai/$1", "sort_order": 0, "enabled": true },
            { "id": 2, "provider": "oai", "alias": "latest", "target": "gpt-test", "sort_order": 0, "enabled": true }
        ]),
    );
    let chat_response = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "hello" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&chat_response).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("codex/latest", false))
        .await
        .expect("pipeline ok");
    assert_eq!(outcome.status, StatusCode::OK);

    let seen = fake.seen.lock().unwrap();
    assert_openai_chat_request(&seen[0], "gpt-test", false);
}

#[tokio::test]
async fn prepared_candidates_survive_snapshot_swap() {
    let fake = Arc::new(FakeUpstream::new(Bytes::new(), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;
    let mut ctx = claude_ctx("claude-test", false);

    let prepared = {
        let cp = state.cp();
        ctx.identity = Some(
            crate::pipeline::auth::authenticate(&cp, &ctx.headers, ctx.query.as_deref()).unwrap(),
        );
        let classified =
            crate::pipeline::classify::classify(&ctx.method, &ctx.path, &ctx.headers, &ctx.body)
                .unwrap();
        ctx.op = Some(classified.op);
        crate::pipeline::candidate::prepare(&cp, &ctx, classified.op).unwrap()
    };
    state
        .snapshot
        .store(Arc::new(ControlPlaneSnapshot::empty(2)));

    let crate::pipeline::candidate::Prepared::Candidates(request) = prepared else {
        panic!("expected routed candidates")
    };
    let request = *request;
    assert_eq!(request.route_name(), Some("to-openai"));
    let admitted = request
        .admit(&state, ctx.identity.as_deref().expect("identity"), false)
        .await
        .expect("owned candidate plan");
    assert_eq!(admitted.candidates.len(), 1);
    assert_eq!(admitted.candidates[0].provider.name, "oai");
    assert_eq!(admitted.candidates[0].upstream_model_id, "gpt-test");
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
