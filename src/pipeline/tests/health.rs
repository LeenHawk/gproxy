//! M4 health integration: breaker trip + failover, 429 credential cooldown
//! with §16.3 edge persistence, and the per-credential rpm budget gate.

use super::*;
use crate::health::CredAdmit;
use crate::health::config::BreakerConfig;
use crate::store::persistence::records::CredentialStatus;
use crate::util::time::unix_now;

/// BUNDLE with several top-level arrays replaced.
fn bundle_multi(overrides: &[(&str, Value)]) -> String {
    let mut v: Value = serde_json::from_str(BUNDLE).expect("bundle json");
    for (key, rows) in overrides {
        v[*key] = rows.clone();
    }
    serde_json::to_string(&v).expect("serialize")
}

fn provider(id: i64, name: &str, settings: Value) -> Value {
    json!({
        "id": id, "name": name, "channel": "openai", "label": null,
        "settings_json": settings, "credential_strategy": "round_robin",
        "proxy_url": null, "tls_fingerprint": null, "enabled": true
    })
}

fn openai_ctx(model: &str) -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({ "model": model, "messages": [{ "role": "user", "content": "hi" }] });
    RequestCtx {
        request_id: "t-h".into(),
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

fn ok_body() -> Bytes {
    let v = json!({
        "id": "chatcmpl-1", "object": "chat.completion", "created": 0, "model": "gpt-a",
        "choices": [{ "index": 0, "message": { "role": "assistant", "content": "hi" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    });
    Bytes::from(serde_json::to_vec(&v).unwrap())
}

/// §16.3 persistence is fire-and-forget (`tokio::spawn`) — poll briefly.
async fn wait_status(
    persistence: &dyn crate::store::persistence::PersistenceBackend,
    credential_id: i64,
    kind: &str,
) -> CredentialStatus {
    for _ in 0..100 {
        let rows = persistence
            .list_credential_statuses(credential_id)
            .await
            .expect("list statuses");
        if let Some(r) = rows.into_iter().find(|r| r.health_kind == kind) {
            return r;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("no `{kind}` status persisted for credential {credential_id}");
}

/// Member 1 (provider `one`) 500s twice → its breaker (consecutive_failures: 2)
/// opens; each request still succeeds by failing over to member 2. The next
/// request skips the open member entirely, and the credential-breaker edge was
/// persisted to `credential_statuses`.
#[tokio::test]
async fn breaker_trips_and_fails_over() {
    let bundle = bundle_multi(&[
        (
            "providers",
            json!([
                provider(
                    1,
                    "one",
                    json!({
                        "base_url": "http://one.local",
                        "circuit_breaker": { "consecutive_failures": 2, "cooldown_secs": 30 }
                    })
                ),
                provider(2, "two", json!({ "base_url": "http://two.local" })),
            ]),
        ),
        (
            "credentials",
            json!([
                { "id": 1, "provider_id": 1, "secret_json": { "api_key": "k1" } },
                { "id": 2, "provider_id": 2, "secret_json": { "api_key": "k2" } },
            ]),
        ),
        ("provider_models", json!([])),
        (
            "routes",
            json!([{ "id": 1, "name": "multi", "strategy": "failover", "enabled": true, "description": null }]),
        ),
        (
            "route_members",
            json!([
                { "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-a", "weight": 100, "tier": 0, "enabled": true },
                { "id": 2, "route_id": 1, "provider_id": 2, "upstream_model_id": "gpt-b", "weight": 50, "tier": 0, "enabled": true },
            ]),
        ),
        ("aliases", json!([])),
        ("routing_rules", json!([])),
        ("provider_rule_sets", json!([])),
    ]);
    let mut fake = FakeUpstream::new(ok_body(), vec![]);
    fake.statuses = vec![
        StatusCode::INTERNAL_SERVER_ERROR, // req1: member 1 fails…
        StatusCode::OK,                    // …fail over to member 2
        StatusCode::INTERNAL_SERVER_ERROR, // req2: member 1 fails again → opens
        StatusCode::OK,
        StatusCode::OK, // req3: member 1 skipped, only member 2 hit
    ];
    let fake = Arc::new(fake);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    for _ in 0..3 {
        let out = crate::pipeline::execute(&state, openai_ctx("multi"))
            .await
            .expect("failover ok");
        assert_eq!(out.status, StatusCode::OK);
    }

    {
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.len(), 5, "request 3 skipped the open member");
        assert!(seen[4].uri.contains("two.local"), "uri: {}", seen[4].uri);
    }

    // §16.3: credential 1's breaker-opened edge was persisted.
    let row = wait_status(state.persistence.as_ref(), 1, "breaker").await;
    assert_eq!(row.channel, "openai");
    let j = row.health_json.expect("health_json");
    assert_eq!(j["state"], "open");
    assert_eq!(j["consecutive_failures"], 2);
    assert_eq!(j["instance_id"], 0);
    assert!(row.last_error.is_some());
}

/// A 429 (no Retry-After) cools the credential for the default 30s — the next
/// request never tries it — and the cooldown edge is persisted. The member is
/// untouched (a rate-limited key says nothing about the member).
#[tokio::test]
async fn rate_limited_cools_credential() {
    let bundle = bundle_multi(&[
        (
            "providers",
            json!([provider(
                1,
                "one",
                json!({ "base_url": "http://one.local" })
            )]),
        ),
        (
            "credentials",
            json!([
                { "id": 1, "provider_id": 1, "secret_json": { "api_key": "k1" } },
                { "id": 2, "provider_id": 1, "secret_json": { "api_key": "k2" } },
            ]),
        ),
        ("provider_models", json!([])),
        (
            "routes",
            json!([{ "id": 1, "name": "multi", "strategy": "failover", "enabled": true, "description": null }]),
        ),
        (
            "route_members",
            json!([{ "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-a", "weight": 100, "tier": 0, "enabled": true }]),
        ),
        ("aliases", json!([])),
        ("routing_rules", json!([])),
        ("provider_rule_sets", json!([])),
    ]);
    let mut fake = FakeUpstream::new(ok_body(), vec![]);
    fake.statuses = vec![
        StatusCode::TOO_MANY_REQUESTS, // req1: cred 1 → 429 → cooldown
        StatusCode::OK,                // …fail over to cred 2
        StatusCode::OK,                // req2: cred 1 skipped, cred 2 only
    ];
    let fake = Arc::new(fake);
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    for _ in 0..2 {
        let out = crate::pipeline::execute(&state, openai_ctx("multi"))
            .await
            .expect("failover ok");
        assert_eq!(out.status, StatusCode::OK);
    }
    assert_eq!(fake.seen.lock().unwrap().len(), 3, "cooled cred skipped");

    let now = unix_now();
    let cfg = BreakerConfig::default();
    assert_eq!(state.health.admit_credential(1, &cfg, now), CredAdmit::No);
    assert_eq!(state.health.admit_credential(2, &cfg, now), CredAdmit::Yes);

    let row = wait_status(state.persistence.as_ref(), 1, "rate_limited").await;
    let j = row.health_json.expect("health_json");
    assert_eq!(j["state"], "cooldown");
    let until = j["open_until"].as_i64().expect("open_until");
    assert!(
        until > now + 20 && until <= now + 31,
        "default 30s: {until}"
    );
}

/// rpm_limit = 1: the second request in the same minute is skipped at the
/// budget gate — no upstream call, and no health failure recorded.
#[tokio::test]
async fn rpm_budget_exhausted_skips_credential() {
    let bundle = bundle_multi(&[
        (
            "providers",
            json!([provider(
                1,
                "one",
                json!({ "base_url": "http://one.local" })
            )]),
        ),
        (
            "credentials",
            json!([{ "id": 1, "provider_id": 1, "secret_json": { "api_key": "k1" }, "rpm_limit": 1 }]),
        ),
        ("provider_models", json!([])),
        (
            "routes",
            json!([{ "id": 1, "name": "multi", "strategy": "failover", "enabled": true, "description": null }]),
        ),
        (
            "route_members",
            json!([{ "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "gpt-a", "weight": 100, "tier": 0, "enabled": true }]),
        ),
        ("aliases", json!([])),
        ("routing_rules", json!([])),
        ("provider_rule_sets", json!([])),
    ]);
    let fake = Arc::new(FakeUpstream::new(ok_body(), vec![]));
    let (state, _dir) = state_with_bundle(Arc::clone(&fake), &bundle).await;

    let out = crate::pipeline::execute(&state, openai_ctx("multi"))
        .await
        .expect("first request within budget");
    assert_eq!(out.status, StatusCode::OK);

    let Err(err) = crate::pipeline::execute(&state, openai_ctx("multi")).await else {
        panic!("second request should be over budget");
    };
    assert!(
        err.to_string().contains("rpm budget"),
        "unexpected error: {err}"
    );
    assert_eq!(
        fake.seen.lock().unwrap().len(),
        1,
        "no second upstream call"
    );

    // A budget skip is not a health failure: the credential stays admitted.
    assert_eq!(
        state
            .health
            .admit_credential(1, &BreakerConfig::default(), unix_now()),
        CredAdmit::Yes
    );
}
