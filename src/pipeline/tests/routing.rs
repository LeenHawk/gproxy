//! Aggregated/scoped route resolution and candidate ownership tests.

use super::*;

fn affinity_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(BUNDLE).expect("bundle json");
    bundle["routes"][0]["strategy"] = json!("round_robin");
    bundle["routes"][0]["settings_json"] = json!({ "affinity": { "enabled": true } });
    bundle["route_members"] = json!([
        { "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-a", "weight": 1, "tier": 0, "enabled": true },
        { "id": 3, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-b", "weight": 1, "tier": 0, "enabled": true },
        { "id": 2, "route_id": 2, "provider_id": 2, "upstream_model_id": "claude-test", "weight": 100, "tier": 0, "enabled": true }
    ]);
    serde_json::to_string(&bundle).expect("serialize affinity bundle")
}

fn affinity_ctx(request_id: &str, session_id: Option<&str>) -> RequestCtx {
    let mut ctx = claude_ctx("claude-test", false);
    ctx.request_id = request_id.to_owned();
    if let Some(session_id) = session_id {
        ctx.headers
            .insert("x-gproxy-session-id", session_id.parse().unwrap());
    }
    ctx
}

fn affinity_response() -> Bytes {
    Bytes::from(
        serde_json::to_vec(&json!({
            "id": "chatcmpl-affinity", "object": "chat.completion", "created": 0, "model": "gpt-a",
            "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        }))
        .unwrap(),
    )
}

fn seen_models(fake: &FakeUpstream) -> Vec<String> {
    fake.seen
        .lock()
        .unwrap()
        .iter()
        .map(|seen| {
            let body: Value = serde_json::from_slice(&seen.body).unwrap();
            body["model"].as_str().unwrap().to_owned()
        })
        .collect()
}

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
        ctx.body_model = classified.body_model;
        crate::pipeline::candidate::prepare(&cp, &ctx, classified.op, None).unwrap()
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
        body_model: None,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx).await.expect("ok");
    assert_eq!(outcome.status, StatusCode::OK);
    let seen = fake.seen.lock().unwrap();
    let up: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(up["model"], "gpt-test", "variant suffix stripped to base");
}

#[tokio::test]
async fn direct_provider_candidates_respect_exact_model_health() {
    let fake = Arc::new(FakeUpstream::new(Bytes::from("{}"), vec![]));
    let (state, _dir) = state_with(fake).await;
    let (blocked, available) = {
        let cp = state.cp();
        let provider = cp.providers_by_name.get("oai").expect("provider");
        (
            crate::pipeline::balance::prepare_provider(&cp, provider, "gpt-test-thinking".into()),
            crate::pipeline::balance::prepare_provider(&cp, provider, "gpt-other".into()),
        )
    };
    state
        .health
        .cool_credential_model(1, "gpt-test", crate::util::time::unix_now() + 60);

    assert!(
        blocked
            .candidates(state.health.as_ref(), state.cache.as_ref(), Some(1))
            .await
            .is_err(),
        "variant resolves to the blocked base model"
    );
    assert_eq!(
        available
            .candidates(state.health.as_ref(), state.cache.as_ref(), Some(1))
            .await
            .expect("different model remains available")
            .len(),
        1
    );
}

#[tokio::test]
async fn route_affinity_prefers_session_header_then_user_key() {
    let fake = Arc::new(FakeUpstream::new(affinity_response(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &affinity_bundle()).await;

    crate::pipeline::execute(&state, affinity_ctx("aff-1", None))
        .await
        .expect("first user request");
    crate::pipeline::execute(&state, affinity_ctx("aff-2", Some("chat-a")))
        .await
        .expect("first session request");
    crate::pipeline::execute(&state, affinity_ctx("aff-3", Some("chat-a")))
        .await
        .expect("sticky session request");
    crate::pipeline::execute(&state, affinity_ctx("aff-4", None))
        .await
        .expect("sticky user request");

    assert_eq!(seen_models(&fake), vec!["gpt-a", "gpt-b", "gpt-b", "gpt-a"]);
    let seen = fake.seen.lock().unwrap();
    assert!(
        seen.iter()
            .all(|request| !request.headers.contains_key("x-gproxy-session-id")),
        "affinity control header must not reach upstream"
    );
}

#[tokio::test]
async fn route_affinity_rebinds_after_pinned_member_fails() {
    let mut upstream = FakeUpstream::new(affinity_response(), vec![]);
    upstream.statuses = vec![
        StatusCode::OK,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::OK,
        StatusCode::OK,
    ];
    let fake = Arc::new(upstream);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &affinity_bundle()).await;

    crate::pipeline::execute(&state, affinity_ctx("rebind-1", None))
        .await
        .expect("initial member");
    crate::pipeline::execute(&state, affinity_ctx("rebind-2", None))
        .await
        .expect("fail over and rebind");
    crate::pipeline::execute(&state, affinity_ctx("rebind-3", None))
        .await
        .expect("new pin");

    assert_eq!(seen_models(&fake), vec!["gpt-a", "gpt-a", "gpt-b", "gpt-b"]);
}
