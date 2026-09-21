#![cfg(not(target_arch = "wasm32"))]
//! Read-your-writes on the management surfaces.
//!
//! An identity write commits its rows, bumps `settings.config_revision` and
//! tells the peers — and reloads nothing, by design. Until this host rebuilt
//! the snapshot afterwards, that meant every write was invisible to the
//! process that made it: a key minted through `/admin/api` answered `401` on
//! the data plane of the very instance that minted it, and a permission
//! granted a moment ago was still refused. Only a restart fixed it.
//!
//! These drive the real router, so what they assert is the middleware's: that
//! a write leaves this instance serving the revision it produced.

mod support;

use gproxy_app::AppConfig;
use http::StatusCode;
use serde_json::json;
use support::{Host, Reply, keyed, post, request};

/// One operator, one provider with a credential behind it, and nothing else:
/// the account that will make the calls does not exist yet.
async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, "k-root", "root", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    host.publish().await;
    host
}

#[tokio::test]
async fn a_key_minted_through_the_admin_api_authenticates_without_a_restart() {
    let host = instance().await;

    let created = host
        .send(keyed(
            post(
                "/admin/api/users",
                json!({ "name": "carol", "role": "user" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    let user_id = created.json()["id"].as_str().unwrap().to_owned();

    let allowed = host
        .send(keyed(
            post(
                "/admin/api/permissions",
                json!({ "userId": user_id, "action": "allow", "modelPattern": "*" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.text());

    let minted = host
        .send(keyed(
            post(
                "/admin/api/api-keys",
                json!({ "userId": user_id, "name": "carol's laptop" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(minted.status, StatusCode::OK, "{}", minted.text());
    let token = minted.json()["token"].as_str().unwrap().to_owned();

    // The next request. No restart, no reload, no thirty-second poll: three
    // writes went through this instance and this instance has to know about
    // them.
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            &token,
        ))
        .await;
    assert_eq!(
        answer.status,
        StatusCode::OK,
        "a key created a moment ago cannot be a 401: {}",
        answer.text()
    );
    assert_eq!(answer.json(), json!({"ok": true}));
}

#[tokio::test]
async fn a_permission_withdrawn_through_the_admin_api_stops_the_next_call() {
    let host = instance().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::allow(&handle, "perm-alice", "alice", None).await;
    host.publish().await;

    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());

    let removed = host
        .send(keyed(
            request(http::Method::DELETE, "/admin/api/permissions/perm-alice"),
            "k-root",
        ))
        .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{}", removed.text());

    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(
        answer.status,
        StatusCode::FORBIDDEN,
        "the rule is gone from the database and has to be gone from the snapshot: {}",
        answer.text()
    );
    assert_eq!(
        host.client.urls().len(),
        1,
        "the refused call reached no upstream"
    );
}

#[tokio::test]
async fn a_portal_key_works_for_the_account_that_just_created_it() {
    let host = Host::with_config(AppConfig::default()).await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    support::allow(&handle, "perm-alice", "alice", None).await;
    host.publish().await;

    let minted = host
        .send(keyed(
            post("/portal/api/keys", json!({ "name": "laptop" })),
            "k-alice",
        ))
        .await;
    assert_eq!(minted.status, StatusCode::OK, "{}", minted.text());
    let token = minted.json()["token"].as_str().unwrap().to_owned();

    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let answer = host
        .send(keyed(
            post("/v1/messages", json!({"model": "test/m1"})),
            &token,
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
}

#[tokio::test]
async fn the_published_revision_moves_with_the_write() {
    let host = instance().await;
    let before = host.app.snapshot().revision();

    let created = host
        .send(keyed(
            post(
                "/admin/api/users",
                json!({ "name": "dave", "role": "user" }),
            ),
            "k-root",
        ))
        .await;
    assert_eq!(created.status, StatusCode::OK, "{}", created.text());
    assert!(
        host.app.snapshot().revision() > before,
        "the instance serves the revision its own write produced"
    );

    // A read moves nothing, and costs no reload.
    let settled = host.app.snapshot().revision();
    let listed = host
        .send(keyed(support::get("/admin/api/users"), "k-root"))
        .await;
    assert_eq!(listed.status, StatusCode::OK);
    assert_eq!(host.app.snapshot().revision(), settled);
}
