//! §6.3 locally-served operations: gateway model lists and local/fallback
//! token counting — no (or failed) upstream involvement.

use super::*;

#[tokio::test]
async fn aggregated_models_lists_aliases_and_routes() {
    let fake = Arc::new(FakeUpstream::new(Bytes::new(), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    let ctx = RequestCtx {
        request_id: "t-m".into(),
        method: Method::GET,
        path: "/v1/models".into(),
        query: None,
        headers,
        body: Bytes::new(),
        mode: RoutingMode::Aggregated,
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    };

    let outcome = crate::pipeline::execute(&state, ctx).await.expect("ok");
    assert_eq!(outcome.status, StatusCode::OK);
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected Full")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    let ids: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    for expected in ["claude-test", "claude-direct", "to-openai", "to-claude"] {
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
    // The oai list-model rule is local; only the permitted cla provider refreshes.
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
}

fn count_ctx(model: &str) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({
        "model": model,
        "messages": [{ "role": "user", "content": "count my tokens please" }]
    });
    RequestCtx {
        request_id: "t-c".into(),
        method: Method::POST,
        path: "/v1/messages/count_tokens".into(),
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

/// §6.3 default: count_tokens routed at an openai-family channel is served
/// locally — claude-shaped response, no upstream call.
#[tokio::test]
async fn count_tokens_on_openai_channel_serves_locally() {
    let fake = Arc::new(FakeUpstream::new(Bytes::new(), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let outcome = crate::pipeline::execute(&state, count_ctx("claude-test"))
        .await
        .expect("ok");

    assert_eq!(outcome.status, StatusCode::OK);
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected Full")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    assert!(v["input_tokens"].as_u64().unwrap() > 0, "body: {v}");
    assert!(fake.seen.lock().unwrap().is_empty(), "no upstream call");
}

/// §6.3 fallback: when every upstream count attempt fails, the gateway answers
/// with a local count instead of a 502.
#[tokio::test]
async fn count_tokens_falls_back_to_local_when_upstream_fails() {
    let mut fake = FakeUpstream::new(Bytes::from_static(b"{}"), vec![]);
    fake.statuses = vec![StatusCode::INTERNAL_SERVER_ERROR];
    let fake = Arc::new(fake);
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    // claude-direct → claude provider → native count passthrough → 500s
    let outcome = crate::pipeline::execute(&state, count_ctx("claude-direct"))
        .await
        .expect("local fallback");

    assert_eq!(outcome.status, StatusCode::OK);
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected Full")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    assert!(v["input_tokens"].as_u64().unwrap() > 0, "body: {v}");
    assert_eq!(fake.seen.lock().unwrap().len(), 1, "upstream was attempted");
}

/// Scoped GET /v1/models at provider `oai`.
fn scoped_models_ctx() -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    RequestCtx {
        request_id: "t-lm".into(),
        method: Method::GET,
        path: "/v1/models".into(),
        query: None,
        headers,
        body: Bytes::new(),
        mode: RoutingMode::Scoped {
            provider: "oai".into(),
        },
        identity: None,
        op: None,
        stream: false,
        route_name: None,
        pending_micros: 0,
    }
}

fn list_ids(b: &Bytes) -> Vec<String> {
    let v: Value = serde_json::from_slice(b).unwrap();
    v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect()
}

/// The provider switch can disable automatic refresh even for a non-local
/// list-model route. Persisted base/variant rows remain visible.
#[tokio::test]
async fn scoped_models_refresh_can_be_disabled() {
    let mut bundle: Value = serde_json::from_str(BUNDLE).unwrap();
    bundle["providers"][0]["settings_json"]["auto_refresh_models"] = json!(false);
    bundle["routing_rules"][0]["implementation"] = json!("passthrough");
    let bundle = serde_json::to_string(&bundle).unwrap();
    let upstream = json!({
        "object": "list",
        "data": [{ "id": "gpt-upstream", "object": "model", "created": 0, "owned_by": "openai" }]
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&upstream).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let outcome = crate::pipeline::execute(&state, scoped_models_ctx())
        .await
        .expect("ok");
    assert_eq!(outcome.status, StatusCode::OK);
    let ResponseBody::Full(b) = outcome.body else {
        panic!("expected Full")
    };
    assert_eq!(
        list_ids(&b),
        ["gpt-test", "gpt-test-thinking"],
        "persisted rows listed"
    );
    assert!(fake.seen.lock().unwrap().is_empty(), "no upstream refresh");

    let persisted = state.persistence.list_provider_models(1).await.unwrap();
    assert_eq!(persisted.len(), 1, "upstream id was not persisted");
}

/// Aggregated listing refreshes upstream and falls back to additions persisted
/// by the previous successful request.
#[tokio::test]
async fn aggregated_models_failure_falls_back_per_provider() {
    let mut bundle: Value = serde_json::from_str(BUNDLE).unwrap();
    bundle["route_permissions"] = json!([
        { "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "oai" }
    ]);
    bundle["routing_rules"] = json!([]);
    let bundle = serde_json::to_string(&bundle).unwrap();
    let upstream = json!({
        "object": "list",
        "data": [{ "id": "gpt-aggregate-persisted", "object": "model", "created": 0, "owned_by": "openai" }]
    });
    let mut fake = FakeUpstream::new(Bytes::from(serde_json::to_vec(&upstream).unwrap()), vec![]);
    fake.statuses = vec![
        StatusCode::OK,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::INTERNAL_SERVER_ERROR,
    ];
    let fake = Arc::new(fake);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    for _ in 0..2 {
        let outcome = crate::pipeline::execute(&state, {
            let mut ctx = scoped_models_ctx();
            ctx.mode = RoutingMode::Aggregated;
            ctx
        })
        .await
        .expect("aggregated catalogue");
        let ResponseBody::Full(body) = outcome.body else {
            panic!("expected Full")
        };
        assert!(
            list_ids(&body).contains(&"oai/gpt-aggregate-persisted".to_owned()),
            "persisted provider model should remain visible"
        );
    }
    assert!(
        fake.seen.lock().unwrap().len() > 1,
        "the second listing must still attempt upstream before fallback"
    );
}
