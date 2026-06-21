//! M7a Task 2 integration: the AuthDead-triggered forced-refresh + retry-once
//! seam (refresh.rs + failover/attempt.rs) and the per-request retry budget.
//! A purpose-built `RefreshChannel` reads its bearer token straight from the
//! opened secret so the test can prove the refreshed token reached the wire.

use super::*;
use std::sync::atomic::AtomicUsize;

use crate::channel::registry::ChannelRegistry;
use crate::channel::{Channel, ChannelError, PrepareCtx, PreparedRequest, TransportKind};
use crate::http::client::UpstreamClient;

/// A claude-speaking channel whose bearer token IS `secret["access_token"]`, so
/// the wire request reflects exactly which secret prepare ran with. `refresh`
/// bumps the token to `"refreshed-token"` (or fails, for the failure test).
struct RefreshChannel {
    refreshes: Arc<AtomicUsize>,
    should_fail: bool,
}

#[async_trait::async_trait]
impl Channel for RefreshChannel {
    fn id(&self) -> &'static str {
        "test_refresh"
    }

    fn provider_family(&self) -> crate::protocol::Provider {
        crate::protocol::Provider::Claude
    }

    fn routing_table(&self) -> crate::channel::routes::RouteList {
        use crate::channel::routes::{cg, pass};
        use crate::protocol::{ContentGenerationKind::ClaudeMessages, Operation::*};
        vec![
            pass(GenerateContent, cg(ClaudeMessages)),
            pass(StreamGenerateContent, cg(ClaudeMessages)),
        ]
    }

    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
        let base_url = ctx
            .provider_settings
            .get("base_url")
            .and_then(Value::as_str)
            .ok_or(ChannelError::MissingSetting("base_url"))?;
        let token = ctx
            .secret
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))?;
        let uri = crate::channel::http_util::join_url(base_url, ctx.path, ctx.query)?;
        let mut req =
            crate::channel::http_util::build_request(ctx.method, uri, HeaderMap::new(), ctx.body)?;
        crate::channel::bulletins::common::inject_bearer(&mut req, token)?;
        Ok(PreparedRequest::new(req))
    }

    // Tests drive refresh exclusively via the AuthDead forced (force=true) path.
    fn needs_refresh(&self, _secret: &Value) -> bool {
        false
    }

    async fn refresh(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        secret: &Value,
    ) -> Result<Value, ChannelError> {
        self.refreshes.fetch_add(1, Ordering::SeqCst);
        if self.should_fail {
            return Err(ChannelError::Unsupported("test refresh fail"));
        }
        let mut next = secret.clone();
        next["access_token"] = Value::String("refreshed-token".into());
        Ok(next)
    }

    fn transport(&self) -> TransportKind {
        TransportKind::Http
    }
}

/// Bundle with one `test_refresh` provider + credential and a claude route/alias
/// so an aggregated `/v1/messages` with model `refresh-model` resolves to it.
/// The `"*"` user-scope permission grant (user 1 / `sk-test`) lets authz pass.
const REFRESH_BUNDLE: &str = r#"{
  "schema_version": 1,
  "orgs": [{ "id": 1, "name": "default", "enabled": true, "description": null }],
  "users": [{ "id": 1, "name": "dev", "org_id": 1, "team_id": null, "password": null, "enabled": true, "is_admin": false }],
  "user_keys": [{ "id": 1, "user_id": 1, "api_key": "sk-test", "label": null, "enabled": true }],
  "route_permissions": [{ "id": 1, "scope": "user", "scope_id": 1, "route_pattern": "*" }],
  "providers": [
    { "id": 1, "name": "rp", "channel": "test_refresh", "label": null, "settings_json": { "base_url": "http://fake.local" }, "credential_strategy": "round_robin", "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "credentials": [
    { "id": 1, "provider_id": 1, "label": null, "secret_json": { "access_token": "initial" }, "proxy_url": null, "tls_fingerprint": null, "enabled": true }
  ],
  "provider_models": [],
  "routes": [{ "id": 1, "name": "to-refresh", "strategy": "failover", "enabled": true, "description": null }],
  "route_members": [{ "id": 1, "route_id": 1, "provider_id": 1, "upstream_model_id": "refresh-model", "weight": 100, "tier": 0, "enabled": true }],
  "aliases": [{ "id": 1, "route_id": 1, "alias": "refresh-model" }],
  "routing_rules": [],
  "rule_sets": [],
  "rules": [],
  "provider_rule_sets": []
}"#;

fn claude_msg_response() -> Bytes {
    let v = json!({
        "id": "msg-1", "type": "message", "role": "assistant", "model": "refresh-model",
        "content": [{ "type": "text", "text": "ok" }],
        "stop_reason": "end_turn", "stop_sequence": null,
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    });
    Bytes::from(serde_json::to_vec(&v).unwrap())
}

/// Read the (NoopCipher-plaintext) secret currently persisted for credential 1.
async fn stored_token(state: &AppState, provider_id: i64, cred_id: i64) -> Option<String> {
    let creds = state.persistence.list_credentials(provider_id).await.ok()?;
    let c = creds.into_iter().find(|c| c.id == cred_id)?;
    c.secret_json
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// 401 → forced refresh → replay the SAME candidate with the refreshed token →
/// 200. Exactly two upstream calls, one refresh, and the rotated token is
/// written back to persistence.
#[tokio::test]
async fn authdead_refresh_retry_succeeds() {
    let mut fake = FakeUpstream::new(claude_msg_response(), vec![]);
    fake.statuses = vec![StatusCode::UNAUTHORIZED, StatusCode::OK];
    let fake = Arc::new(fake);

    let refreshes = Arc::new(AtomicUsize::new(0));
    let reg = Arc::new(ChannelRegistry::with_channel(
        "test_refresh",
        Arc::new(RefreshChannel {
            refreshes: refreshes.clone(),
            should_fail: false,
        }),
    ));
    let (state, _dir) = build_state(
        Arc::clone(&fake),
        REFRESH_BUNDLE,
        &crate::crypto::NoopCipher,
        Arc::new(crate::crypto::NoopCipher),
        reg,
        crate::config::DEFAULT_MAX_ATTEMPTS,
    )
    .await;

    let outcome = crate::pipeline::execute(&state, claude_ctx("refresh-model", false))
        .await
        .expect("pipeline ok after refresh-retry");
    assert_eq!(outcome.status, StatusCode::OK);

    {
        let seen = fake.seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "401 then a single forced-refresh retry");
        assert_eq!(
            seen[0].headers.get("authorization").unwrap(),
            "Bearer initial",
            "first attempt used the original token"
        );
        assert_eq!(
            seen[1].headers.get("authorization").unwrap(),
            "Bearer refreshed-token",
            "retry used the refreshed token"
        );
    }
    assert_eq!(
        refreshes.load(Ordering::SeqCst),
        1,
        "refreshed exactly once"
    );

    // The rotated token was sealed (NoopCipher = plaintext) + written back.
    let mut got = None;
    for _ in 0..200 {
        if let Some(t) = stored_token(&state, 1, 1).await
            && t == "refreshed-token"
        {
            got = Some(t);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert_eq!(
        got.as_deref(),
        Some("refreshed-token"),
        "writeback persisted"
    );
}

/// A failed forced refresh cools the credential and advances — it must NOT
/// replay the candidate. With a single credential and no fallback, the request
/// errors after exactly one upstream call (the 401); the refresh failure does
/// not produce a second.
#[tokio::test]
async fn refresh_failure_skips_credential() {
    let mut fake = FakeUpstream::new(claude_msg_response(), vec![]);
    fake.statuses = vec![StatusCode::UNAUTHORIZED];
    let fake = Arc::new(fake);

    let refreshes = Arc::new(AtomicUsize::new(0));
    let reg = Arc::new(ChannelRegistry::with_channel(
        "test_refresh",
        Arc::new(RefreshChannel {
            refreshes: refreshes.clone(),
            should_fail: true,
        }),
    ));
    let (state, _dir) = build_state(
        Arc::clone(&fake),
        REFRESH_BUNDLE,
        &crate::crypto::NoopCipher,
        Arc::new(crate::crypto::NoopCipher),
        reg,
        crate::config::DEFAULT_MAX_ATTEMPTS,
    )
    .await;

    let result = crate::pipeline::execute(&state, claude_ctx("refresh-model", false)).await;
    assert!(result.is_err(), "401 + failed refresh, no fallback → error");

    assert_eq!(
        fake.seen.lock().unwrap().len(),
        1,
        "no retry after a failed refresh"
    );
    assert_eq!(refreshes.load(Ordering::SeqCst), 1, "refresh was attempted");
}

/// Bundle: one failover route with five members across five `openai` providers,
/// each with its own credential. Used with a tuned `max_attempts` to prove the
/// retry budget caps candidate attempts below the candidate count.
fn five_member_bundle() -> String {
    let providers: Vec<Value> = (1..=5)
        .map(|i| {
            json!({
                "id": i, "name": format!("p{i}"), "channel": "openai", "label": null,
                "settings_json": { "base_url": format!("http://p{i}.local") },
                "credential_strategy": "round_robin", "proxy_url": null,
                "tls_fingerprint": null, "enabled": true
            })
        })
        .collect();
    let credentials: Vec<Value> = (1..=5)
        .map(
            |i| json!({ "id": i, "provider_id": i, "secret_json": { "api_key": format!("k{i}") } }),
        )
        .collect();
    let members: Vec<Value> = (1..=5)
        .map(|i| {
            json!({
                "id": i, "route_id": 1, "provider_id": i, "upstream_model_id": "gpt-a",
                "weight": 100, "tier": 0, "enabled": true
            })
        })
        .collect();
    let mut v: Value = serde_json::from_str(BUNDLE).expect("bundle json");
    v["providers"] = json!(providers);
    v["credentials"] = json!(credentials);
    v["provider_models"] = json!([]);
    v["routes"] = json!([{ "id": 1, "name": "fan", "strategy": "failover", "enabled": true, "description": null }]);
    v["route_members"] = json!(members);
    v["aliases"] = json!([]);
    v["routing_rules"] = json!([]);
    v["provider_rule_sets"] = json!([]);
    serde_json::to_string(&v).expect("serialize")
}

fn openai_fan_ctx() -> RequestCtx {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer sk-test".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = json!({ "model": "fan", "messages": [{ "role": "user", "content": "hi" }] });
    RequestCtx {
        request_id: "t-budget".into(),
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

/// Five candidates all 500, budget = 3: the loop stops after three attempts and
/// returns the last error — the 4th and 5th members are never dialed.
#[tokio::test]
async fn retry_budget_caps_attempts() {
    let mut fake = FakeUpstream::new(Bytes::from_static(b"{\"error\":\"boom\"}"), vec![]);
    fake.statuses = vec![StatusCode::INTERNAL_SERVER_ERROR];
    let fake = Arc::new(fake);

    let reg = Arc::new(ChannelRegistry::with_builtin());
    let (state, _dir) = build_state(
        Arc::clone(&fake),
        &five_member_bundle(),
        &crate::crypto::NoopCipher,
        Arc::new(crate::crypto::NoopCipher),
        reg,
        3,
    )
    .await;

    let result = crate::pipeline::execute(&state, openai_fan_ctx()).await;
    assert!(result.is_err(), "all-500 must error");
    assert_eq!(
        fake.seen.lock().unwrap().len(),
        3,
        "budget stops at 3 despite 5 candidates"
    );
}
