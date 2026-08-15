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

fn conversation_affinity_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(&affinity_bundle()).expect("affinity bundle");
    bundle["routes"][0]["settings_json"] =
        json!({ "affinity": { "enabled": true, "subject": "conversation" } });
    serde_json::to_string(&bundle).expect("serialize conversation affinity bundle")
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

fn conversation_affinity_ctx(
    request_id: &str,
    first_user: &str,
    appended_tail: bool,
) -> RequestCtx {
    let mut ctx = affinity_ctx(request_id, None);
    let mut body: Value = serde_json::from_slice(&ctx.body).expect("request body");
    body["messages"] = if appended_tail {
        json!([
            { "role": "user", "content": first_user },
            { "role": "assistant", "content": "earlier answer" },
            { "role": "user", "content": "follow-up" }
        ])
    } else {
        json!([{ "role": "user", "content": first_user }])
    };
    ctx.body = Bytes::from(serde_json::to_vec(&body).expect("serialize request body"));
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

fn video_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(BUNDLE).expect("bundle");
    bundle["providers"][0]["settings_json"]["endpoints"]["openai_video_create"] =
        json!("http://fake.local/v1/videos");
    bundle["providers"][0]["settings_json"]["endpoints"]["openai_video_retrieve"] =
        json!("http://fake.local/v1/videos/{video_id}");
    bundle["providers"][0]["settings_json"]["endpoints"]["openai_video_content"] =
        json!("http://fake.local/v1/videos/{video_id}/content");
    serde_json::to_string(&bundle).expect("serialize video bundle")
}

fn video_ctx(method: Method, path: &str, body: Value) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    RequestCtx {
        request_id: format!("video-{path}"),
        method,
        path: path.into(),
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

#[tokio::test]
async fn aggregated_video_resource_reuses_creation_route_without_model() {
    let response = Bytes::from_static(
        br#"{"id":"video_123","object":"video","model":"sora-2","status":"queued"}"#,
    );
    let fake = Arc::new(FakeUpstream::new(response, vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &video_bundle()).await;

    crate::pipeline::execute(
        &state,
        video_ctx(
            Method::POST,
            "/v1/videos",
            json!({ "model": "to-openai", "prompt": "a cat" }),
        ),
    )
    .await
    .expect("create video");
    crate::pipeline::execute(
        &state,
        video_ctx(Method::GET, "/v1/videos/video_123", json!({})),
    )
    .await
    .expect("retrieve bound video");

    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].uri, "http://fake.local/v1/videos");
    assert_eq!(seen[1].uri, "http://fake.local/v1/videos/video_123");
    assert!(
        seen[0].headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("multipart/form-data; boundary=gproxy-media-")
    );
}

#[tokio::test]
async fn video_content_preserves_binary_body_and_media_type() {
    let video = Bytes::from_static(b"\0\0\0\x18ftypmp42video-bytes");
    let fake = Arc::new(FakeUpstream::new(Bytes::new(), vec![]).with_responses(vec![
        (
            Bytes::from_static(
                br#"{"id":"video_binary","object":"video","model":"sora-2","status":"completed"}"#,
            ),
            "application/json",
        ),
        (video.clone(), "video/mp4"),
    ]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &video_bundle()).await;

    crate::pipeline::execute(
        &state,
        video_ctx(
            Method::POST,
            "/v1/videos",
            json!({ "model": "to-openai", "prompt": "a cat" }),
        ),
    )
    .await
    .expect("create video");
    let mut content = video_ctx(Method::GET, "/v1/videos/video_binary/content", json!({}));
    content.query = Some("variant=thumbnail&ignored=x".into());
    let outcome = crate::pipeline::execute(&state, content)
        .await
        .expect("download video content");

    assert_eq!(outcome.headers["content-type"], "video/mp4");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("video content should remain buffered bytes");
    };
    assert_eq!(body, video);
    let seen = fake.seen.lock().unwrap();
    assert_eq!(
        seen[1].uri,
        "http://fake.local/v1/videos/video_binary/content?variant=thumbnail"
    );
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
        crate::pipeline::candidate::prepare(
            &cp,
            &ctx,
            classified.op,
            None,
            classified.conversation_fingerprint,
        )
        .unwrap()
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
            .candidates(state.health.as_ref(), state.cache.as_ref(), Some(1), None)
            .await
            .is_err(),
        "variant resolves to the blocked base model"
    );
    assert_eq!(
        available
            .candidates(state.health.as_ref(), state.cache.as_ref(), Some(1), None)
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
async fn route_affinity_uses_conversation_head() {
    let fake = Arc::new(FakeUpstream::new(affinity_response(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &conversation_affinity_bundle()).await;

    crate::pipeline::execute(
        &state,
        conversation_affinity_ctx("conv-a-1", "question A", false),
    )
    .await
    .expect("first conversation A request");
    crate::pipeline::execute(
        &state,
        conversation_affinity_ctx("conv-b-1", "question B", false),
    )
    .await
    .expect("first conversation B request");
    crate::pipeline::execute(
        &state,
        conversation_affinity_ctx("conv-a-2", "question A", true),
    )
    .await
    .expect("conversation A with an appended tail");
    crate::pipeline::execute(
        &state,
        conversation_affinity_ctx("conv-b-2", "question B", true),
    )
    .await
    .expect("conversation B with an appended tail");
    assert_eq!(seen_models(&fake), vec!["gpt-a", "gpt-b", "gpt-a", "gpt-b"]);
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

fn codex_search_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(BUNDLE).expect("bundle");
    bundle["providers"][0]["channel"] = json!("codex");
    bundle["providers"][0]["credential_strategy"] = json!("round_robin");
    bundle["providers"][0]["settings_json"] = json!({
        "endpoints": { "openai_search": "http://fake.local/v1/alpha/search" }
    });
    bundle["credentials"] = json!([
        { "id": 1, "provider_id": 1, "label": "a", "kind": "oauth", "secret_json": { "access_token": "tok-a", "account_id": "acct-a" }, "enabled": true },
        { "id": 3, "provider_id": 1, "label": "b", "kind": "oauth", "secret_json": { "access_token": "tok-b", "account_id": "acct-b" }, "enabled": true },
        { "id": 2, "provider_id": 2, "label": null, "secret_json": { "api_key": "up-key" }, "enabled": true }
    ]);
    bundle["routes"][0]["settings_json"] = json!({ "public_namespace": "openai" });
    serde_json::to_string(&bundle).expect("serialize search bundle")
}

fn codex_search_ctx(request_id: &str, search_id: &str) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    RequestCtx {
        request_id: request_id.into(),
        method: Method::POST,
        path: "/v1/alpha/search".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "id": search_id,
                "model": "to-openai",
                "input": "find this"
            }))
            .unwrap(),
        ),
        mode: RoutingMode::Named {
            name: "openai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    }
}

#[tokio::test]
async fn codex_search_id_hard_binds_one_credential() {
    let response = Bytes::from_static(br#"{"output":"ok","results":[]}"#);
    let fake = Arc::new(FakeUpstream::new(response, vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &codex_search_bundle()).await;

    crate::pipeline::execute(&state, codex_search_ctx("search-1", "session-x"))
        .await
        .expect("first search");
    crate::pipeline::execute(&state, codex_search_ctx("search-2", "session-x"))
        .await
        .expect("bound search");

    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let first = seen[0].headers["authorization"].to_str().unwrap();
    let second = seen[1].headers["authorization"].to_str().unwrap();
    assert_eq!(
        first, second,
        "same search id must not rotate OAuth accounts"
    );
    assert!(
        seen.iter()
            .all(|request| request.uri.ends_with("/v1/alpha/search"))
    );
}

fn codex_realtime_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(&codex_search_bundle()).expect("search bundle");
    bundle["providers"][0]["settings_json"] = json!({
        "endpoints": {
            "openai_realtime_call": "http://fake.local/v1/realtime/calls"
        }
    });
    serde_json::to_string(&bundle).expect("serialize realtime bundle")
}

fn codex_realtime_call_ctx() -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    RequestCtx {
        request_id: "rtc-create".into(),
        method: Method::POST,
        path: "/v1/realtime/calls".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "sdp": "v=offer",
                "model": "to-openai",
                "session": { "type": "realtime" }
            }))
            .unwrap(),
        ),
        mode: RoutingMode::Named {
            name: "openai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    }
}

fn codex_realtime_sideband_ctx() -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    RequestCtx {
        request_id: "rtc-sideband".into(),
        method: Method::GET,
        path: "/v1/realtime".into(),
        query: Some("call_id=rtc-bound".into()),
        headers,
        body: Bytes::new(),
        mode: RoutingMode::Named {
            name: "openai".into(),
        },
        identity: None,
        op: None,
        stream: true,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    }
}

#[tokio::test]
async fn codex_realtime_call_binds_model_and_credential_for_sideband() {
    let upstream = FakeUpstream::new(Bytes::from_static(b"v=answer"), vec![])
        .with_response_content_type("application/sdp")
        .with_response_header("location", "/v1/realtime/calls/calls/rtc-bound");
    let fake = Arc::new(upstream);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &codex_realtime_bundle()).await;

    let outcome = crate::pipeline::execute(&state, codex_realtime_call_ctx())
        .await
        .expect("create realtime call");
    assert_eq!(outcome.status, StatusCode::OK);

    let _session = crate::pipeline::realtime::open(&state, codex_realtime_sideband_ctx())
        .await
        .expect("bound sideband");

    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let call_auth = seen[0].headers["authorization"].to_str().unwrap();
    let sideband_auth = seen[1].headers["authorization"].to_str().unwrap();
    assert_eq!(
        call_auth, sideband_auth,
        "sideband must reuse the call credential"
    );
    let call_body: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(call_body["session"]["model"], "gpt-test");
    assert!(
        call_body.get("model").is_none(),
        "public route model must not leak top-level"
    );
    assert_eq!(seen[0].headers["accept"], "application/sdp");
    assert_eq!(
        seen[1].uri,
        "wss://api.openai.com/v1/realtime?call_id=rtc-bound"
    );
}

fn codex_responses_bundle() -> String {
    let mut bundle: Value = serde_json::from_str(&codex_search_bundle()).expect("search bundle");
    bundle["providers"][0]["settings_json"] = json!({
        "endpoints": { "openai_responses": "http://fake.local/v1/responses" }
    });
    serde_json::to_string(&bundle).expect("serialize responses bundle")
}

fn codex_responses_ctx(request_id: &str, turn_state: Option<&str>) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    if let Some(turn_state) = turn_state {
        headers.insert("x-codex-turn-state", turn_state.parse().unwrap());
    }
    RequestCtx {
        request_id: request_id.into(),
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers,
        body: Bytes::from(
            serde_json::to_vec(&json!({
                "model": "to-openai",
                "input": "hello",
                "stream": true
            }))
            .unwrap(),
        ),
        mode: RoutingMode::Named {
            name: "openai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        body_model: None,
        route_name: None,
        pending_micros: 0,
    }
}

#[tokio::test]
async fn codex_returned_turn_state_binds_next_request_to_same_credential() {
    let upstream = FakeUpstream::new(Bytes::new(), vec![])
        .with_response_header("x-codex-turn-state", "turn-state-1")
        .with_response_header("x-codex-primary-used-percent", "96")
        .with_response_header("x-codex-credits-balance", "0")
        .with_response_header("x-codex-routing-hint", "internal-route");
    let fake = Arc::new(upstream);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &codex_responses_bundle()).await;

    let first = crate::pipeline::execute(&state, codex_responses_ctx("turn-1", None))
        .await
        .expect("first responses request");
    assert_eq!(first.headers["x-codex-turn-state"], "turn-state-1");
    assert!(first.headers.get("x-codex-primary-used-percent").is_none());
    assert!(first.headers.get("x-codex-credits-balance").is_none());
    assert!(first.headers.get("x-codex-routing-hint").is_none());
    let _second =
        crate::pipeline::execute(&state, codex_responses_ctx("turn-2", Some("turn-state-1")))
            .await
            .expect("bound responses request");

    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(
        seen[0].headers["authorization"], seen[1].headers["authorization"],
        "server-issued turn state must bind the next request before rotation"
    );
    assert_eq!(seen[1].headers["x-codex-turn-state"], "turn-state-1");
}
