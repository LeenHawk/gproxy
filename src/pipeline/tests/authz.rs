//! M3 authz integration: enforcement in the aggregated arm + permission-
//! filtered model listing. Each test builds its own state (fresh MemoryCache
//! + per-test bundle), so counters and grants never leak between tests.

use super::*;
use crate::pipeline::error::PipelineError;

fn chat_ok() -> Bytes {
    let body = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-test",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "ok" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    Bytes::from(serde_json::to_vec(&body).unwrap())
}

/// `expect_err` without requiring `ExecOutcome: Debug`.
async fn exec_err(state: &AppState, ctx: RequestCtx) -> PipelineError {
    match crate::pipeline::execute(state, ctx).await {
        Err(e) => e,
        Ok(_) => panic!("expected pipeline error"),
    }
}

fn models_ctx(api_key: &str) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        format!("Bearer {api_key}").parse().unwrap(),
    );
    RequestCtx {
        request_id: "t-az".into(),
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
    }
}

#[tokio::test]
async fn no_permission_user_403() {
    let fake = Arc::new(FakeUpstream::new(chat_ok(), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let err = exec_err(&state, claude_ctx_as("sk-noperm", "claude-test", false)).await;
    assert!(matches!(err, PipelineError::Forbidden), "got {err:?}");
    assert!(fake.seen.lock().unwrap().is_empty(), "no upstream call");
}

#[tokio::test]
async fn rpm_limit_trips() {
    let bundle = bundle_with(
        "rate_limits",
        json!([{ "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "*",
                 "rpm": 1, "rpd": null, "total_tokens": null }]),
    );
    let fake = Arc::new(FakeUpstream::new(chat_ok(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    crate::pipeline::execute(&state, claude_ctx("claude-test", false))
        .await
        .expect("first request under limit");
    let err = exec_err(&state, claude_ctx("claude-test", false)).await;
    assert!(
        matches!(err, PipelineError::RateLimited { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn quota_exceeded_429() {
    let bundle = bundle_with(
        "quotas",
        json!([{ "id": 1, "scope": "user", "scope_id": 1,
                 "quota_total": "1.00", "cost_used": "2.00" }]),
    );
    let fake = Arc::new(FakeUpstream::new(chat_ok(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let err = exec_err(&state, claude_ctx("claude-test", false)).await;
    assert!(matches!(err, PipelineError::QuotaExceeded), "got {err:?}");
    assert!(fake.seen.lock().unwrap().is_empty(), "no upstream call");
}

#[tokio::test]
async fn models_list_filtered() {
    let fake = Arc::new(FakeUpstream::new(Bytes::new(), vec![]));
    let (state, _dir) = state_with(Arc::clone(&fake)).await;

    let list = |outcome: crate::pipeline::ExecOutcome| -> usize {
        let ResponseBody::Full(b) = outcome.body else {
            panic!("expected Full")
        };
        let v: Value = serde_json::from_slice(&b).unwrap();
        v["data"].as_array().unwrap().len()
    };

    let denied = crate::pipeline::execute(&state, models_ctx("sk-noperm"))
        .await
        .expect("listing itself is allowed");
    assert_eq!(denied.status, StatusCode::OK);
    assert_eq!(list(denied), 0, "noperm sees nothing");

    let allowed = crate::pipeline::execute(&state, models_ctx("sk-test"))
        .await
        .expect("ok");
    assert!(list(allowed) >= 2, "grant holder sees aliases + routes");
}

#[tokio::test]
async fn provider_model_child_permission_filters_aggregate_catalogue() {
    let bundle = bundle_with(
        "route_permissions",
        json!([{ "id": 1, "scope": "user", "scope_id": 1,
                 "route_pattern": "oai/gpt-test" }]),
    );
    // This test exercises permission filtering of a live catalogue. The shared
    // fixture routes oai ListModels locally, so opt this case into passthrough.
    let mut bundle: Value = serde_json::from_str(&bundle).unwrap();
    bundle["routing_rules"][0]["implementation"] = json!("passthrough");
    let bundle = serde_json::to_string(&bundle).unwrap();
    let upstream = json!({
        "object": "list",
        "data": [
            { "id": "gpt-test", "object": "model", "created": 0, "owned_by": "openai" },
            { "id": "gpt-secret", "object": "model", "created": 0, "owned_by": "openai" }
        ]
    });
    let fake = Arc::new(FakeUpstream::new(
        Bytes::from(serde_json::to_vec(&upstream).unwrap()),
        vec![],
    ));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let outcome = crate::pipeline::execute(&state, models_ctx("sk-test"))
        .await
        .expect("filtered list");
    let ResponseBody::Full(body) = outcome.body else {
        panic!("expected Full")
    };
    let value: Value = serde_json::from_slice(&body).unwrap();
    let ids: Vec<&str> = value["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["oai/gpt-test"]);
    assert_eq!(fake.seen.lock().unwrap().len(), 1, "only oai is refreshed");
}

#[tokio::test]
async fn provider_model_child_permission_controls_actual_calls() {
    let bundle = bundle_with(
        "route_permissions",
        json!([{ "id": 1, "scope": "user", "scope_id": 1,
                 "route_pattern": "oai/gpt-test" }]),
    );
    let fake = Arc::new(FakeUpstream::new(chat_ok(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    crate::pipeline::execute(&state, claude_ctx("oai/gpt-test", false))
        .await
        .expect("exact child grant");
    let err = exec_err(&state, claude_ctx("oai/gpt-secret", false)).await;
    assert!(matches!(err, PipelineError::Forbidden), "got {err:?}");

    let mut scoped_allowed = claude_ctx("gpt-test", false);
    scoped_allowed.mode = RoutingMode::Scoped {
        provider: "oai".into(),
    };
    crate::pipeline::execute(&state, scoped_allowed)
        .await
        .expect("scoped exact child grant");
    let mut scoped_denied = claude_ctx("gpt-secret", false);
    scoped_denied.mode = RoutingMode::Scoped {
        provider: "oai".into(),
    };
    let err = exec_err(&state, scoped_denied).await;
    assert!(matches!(err, PipelineError::Forbidden), "got {err:?}");
    assert_eq!(
        fake.seen.lock().unwrap().len(),
        2,
        "denied calls never hit upstream"
    );
}

/// Regression: `route.enabled = false` used to be ignored by the snapshot —
/// the route stayed routable and listed. It must 404 (route name AND alias)
/// and vanish from the aggregated model list.
#[tokio::test]
async fn disabled_route_is_unroutable_and_unlisted() {
    let bundle = bundle_with(
        "routes",
        json!([
            { "id": 1, "name": "to-openai", "strategy": "failover", "enabled": true, "description": null },
            { "id": 2, "name": "to-claude", "strategy": "failover", "enabled": false, "description": null }
        ]),
    );
    let fake = Arc::new(FakeUpstream::new(chat_ok(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let err = exec_err(&state, claude_ctx("to-claude", false)).await;
    assert!(matches!(err, PipelineError::UnknownRoute(_)), "got {err:?}");
    // alias "claude-direct" points at the disabled route → gone with it
    let err = exec_err(&state, claude_ctx("claude-direct", false)).await;
    assert!(matches!(err, PipelineError::UnknownRoute(_)), "got {err:?}");
    assert!(fake.seen.lock().unwrap().is_empty(), "no upstream call");

    let listed = crate::pipeline::execute(&state, models_ctx("sk-test"))
        .await
        .expect("ok");
    let ResponseBody::Full(b) = listed.body else {
        panic!("expected Full")
    };
    let v: Value = serde_json::from_slice(&b).unwrap();
    let ids: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"to-openai"), "enabled route listed: {ids:?}");
    assert!(
        !ids.contains(&"to-claude") && !ids.contains(&"claude-direct"),
        "disabled route/alias leaked into {ids:?}"
    );
}
