#![cfg(not(target_arch = "wasm32"))]
//! The data plane end to end: a `Caller` goes in, the engine runs with exactly
//! what admission decided, and the charges come back attached to the outcome.
//!
//! Every upstream here is scripted, so "nothing was sent" is an assertion the
//! client can answer rather than an absence of evidence.

mod support;

use gproxy_app::{AppConfig, AppError, AppPublicationUrl, DataPlaneRequest, RequestedView};
use gproxy_core::PublicationUrl;
use gproxy_store::entity::{identity::membership_role::MembershipRole, usage::usage_record};
use http::StatusCode;
use sea_orm::EntityTrait;
use serde_json::json;
use support::Reply;

/// One provider `p1` serving `m1`, one shared credential, and `alice` holding
/// a key that may reach it.
async fn one_provider() -> (
    gproxy_app::App<sea_orm::DatabaseConnection>,
    std::sync::Arc<support::ScriptClient>,
) {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::allow(&handle, "p-alice", "alice", None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c-shared", "p1", None, None, None).await;
    support::publish(&app).await;
    (app, client)
}

fn request(model: &str) -> DataPlaneRequest {
    let (parts, body) = support::parts(json!({"model": model}));
    DataPlaneRequest::new("req-1", support::generate(), parts, body)
}

/// Drain the response and its settlement, which is what makes the usage row
/// appear. The capture these tests do not look at is exercised in
/// `tests/capture.rs`.
async fn finish(outcome: gproxy_app::CallOutcome) -> serde_json::Value {
    let gproxy_app::CallOutcome {
        execution,
        admitted,
        ..
    } = outcome;
    let (response, usage) = execution.into_parts();
    let body = support::read_json(response.body).await;
    usage.await.unwrap();
    // The host holds the leases until the response is written; this is that
    // moment.
    drop(admitted);
    body
}

async fn usage_rows(
    app: &gproxy_app::App<sea_orm::DatabaseConnection>,
) -> Vec<usage_record::Model> {
    app.gproxy()
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_permitted_key_reaches_the_upstream_and_is_metered() {
    let (app, client) = one_provider().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app.call(&caller, request("test/m1")).await.unwrap();
    assert_eq!(outcome.execution.response().status, StatusCode::OK);
    assert_eq!(finish(outcome).await, json!({"ok": true}));

    assert_eq!(client.urls(), ["https://p1.example/v1/responses"]);
    let rows = usage_rows(&app).await;
    assert_eq!(rows.len(), 1, "one request, one usage summary");
    assert_eq!(rows[0].request_id, "req-1");
    assert_eq!(
        rows[0].user_id.as_deref(),
        Some("alice"),
        "the usage row names the person admission attributed it to"
    );
    assert_eq!(rows[0].api_key_id.as_deref(), Some("k-alice"));
    assert_eq!(
        rows[0].model, "test/m1",
        "the model the caller asked for, not the upstream one"
    );
}

#[tokio::test]
async fn a_permission_narrows_the_plan_to_the_providers_it_names() {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    // Both providers serve `m1`; only `p2` is permitted. The providers are
    // written first because a provider-scoped rule references one.
    support::provider(&handle, "p1", &["m1"]).await;
    support::provider(&handle, "p2", &["m1"]).await;
    support::allow(&handle, "p-alice", "alice", Some("p2")).await;
    support::credential(&handle, "c-1", "p1", None, None, None).await;
    support::credential(&handle, "c-2", "p2", None, None, None).await;
    support::publish(&app).await;

    client.script(vec![Reply::Http(StatusCode::OK, json!({"from": "p2"}))]);
    let caller = support::caller_for(&app, "k-alice").await;
    let outcome = app.call(&caller, request("test/m1")).await.unwrap();
    assert_eq!(finish(outcome).await, json!({"from": "p2"}));
    assert_eq!(
        client.urls(),
        ["https://p2.example/v1/responses"],
        "the provider the permission excluded was never a target"
    );
}

#[tokio::test]
async fn a_caller_with_no_permission_is_refused_before_anything_is_sent() {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "bob", "user").await;
    support::api_key(&handle, "k-bob", "bob", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c-shared", "p1", None, None, None).await;
    support::publish(&app).await;

    let caller = support::caller_for(&app, "k-bob").await;
    let error = app.call(&caller, request("test/m1")).await.unwrap_err();
    assert_eq!(error.status_code(), 403);
    assert_eq!(error.code(), "forbidden");
    assert!(
        client.urls().is_empty(),
        "a refused caller must not cost an upstream call"
    );
}

#[tokio::test]
async fn a_team_bound_key_cannot_spend_another_organizations_credential() {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::organization(&handle, "acme").await;
    support::organization(&handle, "globex").await;
    support::team(&handle, "core", "acme").await;
    support::api_key(&handle, "k-team", "alice", None, Some("core")).await;
    support::allow(&handle, "p-alice", "alice", None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    // The only credential belongs to the other organization.
    support::credential(&handle, "c-globex", "p1", None, None, Some("globex")).await;
    support::publish(&app).await;

    let caller = support::caller_for(&app, "k-team").await;
    let error = app.call(&caller, request("test/m1")).await.unwrap_err();
    assert_eq!(
        error.status_code(),
        503,
        "permitted the provider, but with nothing left to spend on it: {error}"
    );
    assert_eq!(error.code(), "no_usable_target");
    assert!(client.urls().is_empty());

    // The same key reaches its own organization's credential.
    support::credential(&handle, "c-acme", "p1", None, None, Some("acme")).await;
    support::publish(&app).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-team").await;
    let outcome = app.call(&caller, request("test/m1")).await.unwrap();
    assert_eq!(finish(outcome).await, json!({"ok": true}));
}

#[tokio::test]
async fn a_spent_budget_is_refused_as_429() {
    let (app, client) = one_provider().await;
    support::spent_budget(app.gproxy(), "q-alice", "user", "alice").await;
    support::publish(&app).await;

    let caller = support::caller_for(&app, "k-alice").await;
    let error = app.call(&caller, request("test/m1")).await.unwrap_err();
    assert_eq!(error.status_code(), 429);
    assert_eq!(error.code(), "budget_exhausted");
    assert!(
        client.urls().is_empty(),
        "an exhausted budget is checked before the first attempt"
    );
}

#[tokio::test]
async fn a_rate_limit_of_one_admits_then_refuses_with_a_retry_hint() {
    let (app, client) = one_provider().await;
    // An hour-long window, so the two calls below cannot straddle a boundary.
    support::rate_limit(app.gproxy(), "rl-alice", "alice", 1, 3600).await;
    support::publish(&app).await;

    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;
    let outcome = app.call(&caller, request("test/m1")).await.unwrap();
    assert_eq!(finish(outcome).await, json!({"ok": true}));

    let error = app.call(&caller, request("test/m1")).await.unwrap_err();
    match error {
        AppError::RateLimited { retry_after_ms } => {
            let hint = retry_after_ms.expect("a window knows when it reopens");
            assert!(
                hint > 0 && hint <= 3_600_000,
                "the hint is the rest of the window, got {hint}"
            );
        }
        other => panic!("expected a rate-limit refusal, got {other}"),
    }
    assert_eq!(
        client.urls().len(),
        1,
        "the second request never reached an upstream"
    );
}

#[tokio::test]
async fn a_refused_request_gives_back_what_earlier_steps_charged() {
    // The limit allows one request per hour; the only call fails in the
    // engine, so the window must be free again afterwards.
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::allow(&handle, "p-alice", "alice", None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    // No credential at all: the plan resolves to nothing usable.
    support::rate_limit(&handle, "rl-alice", "alice", 1, 3600).await;
    support::publish(&app).await;

    let caller = support::caller_for(&app, "k-alice").await;
    let error = app.call(&caller, request("test/m1")).await.unwrap_err();
    assert_eq!(error.status_code(), 503);

    // Now give it a credential: the second request is admitted, which it
    // could not be if the first had kept the only slot in the window.
    support::credential(&handle, "c-shared", "p1", None, None, None).await;
    support::publish(&app).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;
    let outcome = app.call(&caller, request("test/m1")).await.unwrap();
    assert_eq!(finish(outcome).await, json!({"ok": true}));
}

#[tokio::test]
async fn the_gateway_session_header_never_reaches_an_upstream() {
    let (app, client) = one_provider().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let (mut parts, body) = support::parts(json!({"model": "test/m1"}));
    parts.headers.insert(
        gproxy_sdk::GATEWAY_SESSION_HEADER,
        http::HeaderValue::from_static("ours"),
    );
    let caller = support::caller_for(&app, "k-alice").await;
    let outcome = app
        .call(
            &caller,
            DataPlaneRequest::new("req-1", support::generate(), parts, body),
        )
        .await
        .unwrap();
    // The identity was derived from it before it was stripped.
    assert_eq!(
        outcome
            .admitted
            .session
            .as_ref()
            .map(|session| session.id.as_str()),
        Some("ours")
    );
    finish(outcome).await;
    assert_eq!(client.urls().len(), 1);
}

// ------------------------------------------------------------- services ----

/// `boss` administers `acme`, `member` belongs to it, and the only credential
/// of `p1` is the organization's.
async fn service_world() -> gproxy_app::App<sea_orm::DatabaseConnection> {
    let (app, _) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "root", "admin").await;
    support::person(&handle, "boss", "user").await;
    support::person(&handle, "member", "user").await;
    support::organization(&handle, "acme").await;
    support::org_member(&handle, "acme", "boss", MembershipRole::Admin).await;
    support::org_member(&handle, "acme", "member", MembershipRole::Member).await;
    support::api_key(&handle, "k-root", "root", None, None).await;
    support::api_key(&handle, "k-boss", "boss", Some("acme"), None).await;
    support::api_key(&handle, "k-member", "member", Some("acme"), None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c-acme", "p1", None, None, Some("acme")).await;
    support::usage_row(&handle, "past-1", "member", 41).await;
    support::publish(&app).await;
    app
}

fn service(view: RequestedView) -> gproxy_app::ServiceRequestIn {
    let (parts, body) = support::service_parts("/api/profile");
    gproxy_app::ServiceRequestIn {
        request_id: "svc-1".into(),
        parts,
        body,
        view,
        provider_id: "p1".into(),
    }
}

#[tokio::test]
async fn a_member_gets_the_caller_view_rendered_from_their_own_usage() {
    let app = service_world().await;
    let caller = support::caller_for(&app, "k-member").await;
    let response = app
        .call_service(&caller, service(RequestedView::Caller))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    let body = support::read_json(response.body).await;
    assert_eq!(body["role"], "member");
    assert_eq!(body["view"], "caller");
    assert_eq!(
        body["identity"], "user:member",
        "the caller view is identified by the scope, never by a credential"
    );
    assert_eq!(
        body["input_tokens"], 41,
        "the user id a model request would attribute to is the one the view reads"
    );
    assert_eq!(body["path"], "/api/profile");
}

#[tokio::test]
async fn a_member_asking_for_the_pool_is_refused() {
    let app = service_world().await;
    let caller = support::caller_for(&app, "k-member").await;
    for view in [
        RequestedView::Pool,
        RequestedView::Credential("c-acme".into()),
    ] {
        let error = app.call_service(&caller, service(view)).await.unwrap_err();
        assert_eq!(error.status_code(), 403);
        assert_eq!(error.code(), "forbidden");
    }
}

#[tokio::test]
async fn an_organization_admin_and_an_instance_admin_both_reach_the_pool() {
    let app = service_world().await;
    for key in ["k-boss", "k-root"] {
        let caller = support::caller_for(&app, key).await;
        let response = app
            .call_service(&caller, service(RequestedView::Pool))
            .await
            .unwrap();
        let body = support::read_json(response.body).await;
        assert_eq!(body["role"], "admin", "{key}");
        assert_eq!(body["view"], "pool");
        assert_eq!(
            body["identity"], "p1:c-acme",
            "the pool identity names the provider and the exact credential set"
        );
        assert_eq!(body["account"], "c-acme");
    }
}

#[tokio::test]
async fn a_credential_view_can_only_name_a_credential_the_caller_reaches() {
    let app = service_world().await;
    let caller = support::caller_for(&app, "k-boss").await;
    let response = app
        .call_service(&caller, service(RequestedView::Credential("c-acme".into())))
        .await
        .unwrap();
    let body = support::read_json(response.body).await;
    assert_eq!(body["view"], "credential:c-acme");

    let error = app
        .call_service(
            &caller,
            service(RequestedView::Credential("c-elsewhere".into())),
        )
        .await
        .unwrap_err();
    assert_eq!(error.status_code(), 404);

    // And an unknown provider is a 404 rather than an empty pool.
    let mut request = service(RequestedView::Pool);
    request.provider_id = "nope".into();
    let error = app.call_service(&caller, request).await.unwrap_err();
    assert_eq!(error.status_code(), 404);
}

// ---------------------------------------------------------- publications ---

#[test]
fn a_publication_url_needs_a_configured_origin() {
    let reference = gproxy_core::PublicationRef {
        id: "pub-1",
        scope: "user:alice",
        mime: Some("image/png"),
        expires_at_ms: None,
    };
    let configured = AppPublicationUrl::from_config(&AppConfig {
        public_base_url: Some("https://gproxy.example.com/".into()),
        ..AppConfig::default()
    });
    assert_eq!(
        configured.url_for(&reference).as_deref(),
        Some("https://gproxy.example.com/publications/pub-1")
    );
    // Without one, publication is refused before any body is written rather
    // than handed a link that resolves nowhere.
    assert_eq!(
        AppPublicationUrl::from_config(&AppConfig::default()).url_for(&reference),
        None
    );
}

#[tokio::test]
async fn an_unknown_publication_is_not_found() {
    let (app, _) = support::app().await;
    // `Publication` carries bytes and has no `Debug`, so the refusal is
    // matched rather than unwrapped.
    match app.read_publication("nope").await {
        Err(error) => {
            assert_eq!(error.status_code(), 404);
            assert_eq!(error.code(), "not_found");
        }
        Ok(_) => panic!("an id nothing was published under must not resolve"),
    }

    let error = app.delete_publication("nope").await.unwrap_err();
    assert_eq!(error.status_code(), 404);
}

// ------------------------------------------------------------- the handle --

#[tokio::test]
async fn an_app_starts_blind_and_publishes_what_it_reads() {
    let (app, _) = support::app().await;
    assert_eq!(
        app.data().revision,
        i64::MIN,
        "no key is known until the first refresh"
    );
    let revision = app.reload_all().await.unwrap();
    assert_eq!(app.data().revision, revision);

    // A write moves the durable revision; the snapshot follows it.
    support::person(app.gproxy(), "alice", "user").await;
    support::api_key(app.gproxy(), "k-alice", "alice", None, None).await;
    let next = app
        .gproxy()
        .store()
        .commit_revision(vec![])
        .await
        .unwrap()
        .revision;
    assert!(app.data().users.is_empty(), "not until it is refreshed");
    assert_eq!(app.refresh().await.unwrap(), next);
    assert!(app.data().users.contains_key("alice"));
    assert_eq!(app.data().keys.len(), 1);
}

#[tokio::test]
async fn app_preserves_inferred_affinity_and_managed_reset_selection() {
    use gproxy_sdk::dto::{CredentialPatch, ProviderPatch};
    use gproxy_store::entity::limits::credential_cycle::{self, CycleBoundary, CycleOpening};
    use sea_orm::Set;
    let (app, client) = one_provider().await;
    let handle = app.gproxy();
    support::credential(handle, "c-later", "p1", None, None, None).await;
    handle
        .manage()
        .providers()
        .update(
            "p1",
            ProviderPatch {
                config: Some(
                    json!({"credential_strategy":"earliest_reset", "session_affinity":true}),
                ),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    for (id, delay) in [("c-shared", 600_000), ("c-later", 3_600_000)] {
        handle
            .manage()
            .credentials()
            .update(
                id,
                CredentialPatch {
                    metadata: Some(json!({"observed_quota":true})),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        handle
            .store()
            .credential_cycles()
            .create_many(vec![credential_cycle::ActiveModel {
                id: Set(format!("cycle-{id}")),
                credential_id: Set(id.into()),
                open_key: Set(Some(credential_cycle::open_key(id, "5h"))),
                window_id: Set("5h".into()),
                dimension_id: Set(Some("5h".into())),
                scope: Set(json!("all")),
                starts_at_ms: Set(now),
                ends_at_ms: Set(Some(now + delay)),
                boundary: Set(CycleBoundary::Observed),
                opened_by: Set(CycleOpening::FirstUse),
                cost_usd: Set(gproxy_store::FixedDecimal::ZERO),
                sample_at_ms: Set(Some(now)),
                ..Default::default()
            }])
            .await
            .unwrap();
    }
    support::publish(&app).await;
    client.script(
        (0..3)
            .map(|_| Reply::Http(StatusCode::OK, json!({"ok":true})))
            .collect(),
    );
    let caller = support::caller_for(&app, "k-alice").await;
    let first = json!({"model":"test/m1", "input":[{"role":"user", "content":"hello"}]});
    let mut second = first.clone();
    second["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"assistant", "content":"hi"}));
    second["input"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"user", "content":"next"}));
    let mut ids = Vec::new();
    for (index, body) in [
        first,
        second,
        json!({"model":"test/m1", "input":"different question"}),
    ]
    .into_iter()
    .enumerate()
    {
        // After the first request, c-later becomes the earliest reset candidate.
        if index == 1 {
            handle
                .store()
                .credential_cycles()
                .update_many(vec![credential_cycle::ActiveModel {
                    id: Set("cycle-c-later".into()),
                    ends_at_ms: Set(Some(now + 300_000)),
                    sample_at_ms: Set(Some(now + 1)),
                    ..Default::default()
                }])
                .await
                .unwrap();
            handle
                .cache()
                .delete(&gproxy_core::keys::credential_reset_observations("c-later"))
                .await
                .unwrap();
        }
        let (parts, body) = support::parts(body);
        let outcome = app
            .call(
                &caller,
                DataPlaneRequest::new(format!("r-{index}"), support::generate(), parts, body),
            )
            .await
            .unwrap();
        let session = outcome.admitted.session.as_ref().unwrap();
        assert_eq!(
            session.source,
            gproxy_core::SessionSource::ConversationFingerprint
        );
        ids.push(session.id.clone());
        finish(outcome).await;
    }
    assert_eq!(ids[0], ids[1]);
    assert_ne!(ids[0], ids[2]);
    assert_eq!(
        *client.authorizations.lock().unwrap(),
        ["Bearer k-c-shared", "Bearer k-c-shared", "Bearer k-c-later"]
    );
}
