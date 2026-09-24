#![cfg(not(target_arch = "wasm32"))]
mod support;

use http::StatusCode;
use sea_orm::ConnectionTrait;
use serde_json::{Value, json};
use support::{Host, get, keyed, post};

async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "root", "admin").await;
    support::api_key(&handle, "root-key", "root", None, None).await;
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "alice-key", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    host.publish().await;
    host
}
async fn revision(host: &Host) -> i64 {
    host.send(keyed(get("/admin/api/settings"), "root-key"))
        .await
        .json()["instance"]["configRevision"]
        .as_i64()
        .unwrap()
}
async fn create(host: &Host, body: Value) -> support::Answer {
    host.send(keyed(post("/admin/api/api-keys", body), "root-key"))
        .await
}

#[tokio::test]
async fn key_and_budget_are_one_revision_and_effective_before_the_key_is_returned() {
    let host = instance().await;
    let before = revision(&host).await;
    let result = create(&host, json!({"userId":"root","name":"bounded","budget":{"windowKey":"Monthly budget","limitValue":"0","period":"1m"}})).await;
    assert_eq!(result.status, StatusCode::OK, "{}", result.text());
    let row = result.json();
    let id = row["id"].as_str().unwrap();
    assert_eq!(revision(&host).await, before + 1);
    let budgets = host
        .send(keyed(
            get(&format!("/admin/api/quotas?ownerKind=api_key&ownerId={id}")),
            "root-key",
        ))
        .await;
    let items = budgets.json()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["ownerId"], id);
    assert_eq!(items[0]["metric"], "cost");
    assert_eq!(items[0]["unit"], "USD");
    assert_eq!(items[0]["limitValue"], "0");
    assert!(
        host.handle()
            .core()
            .snapshot()
            .budgets
            .iter()
            .any(|budget| budget.owner.kind == "api_key" && budget.owner.id == id)
    );
    let response = host
        .send(keyed(
            post("/v1/responses", json!({"model":"p1/m1","input":"hello"})),
            row["token"].as_str().unwrap(),
        ))
        .await;
    assert_eq!(
        response.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        response.text()
    );
}

#[tokio::test]
async fn invalid_budget_creates_neither_key_nor_revision() {
    let host = instance().await;
    let before = revision(&host).await;
    for budget in [
        json!({"limitValue":"-1"}),
        json!({"limitValue":"10","period":"custom"}),
        json!({"limitValue":"not-money"}),
    ] {
        let result = create(
            &host,
            json!({"userId":"alice","name":"invalid-budget","budget":budget}),
        )
        .await;
        assert_eq!(result.status, StatusCode::BAD_REQUEST, "{}", result.text());
    }
    assert_eq!(revision(&host).await, before);
    let keys = host
        .send(keyed(
            get("/admin/api/api-keys?search=invalid-budget"),
            "root-key",
        ))
        .await;
    assert_eq!(keys.json()["total"], 0);
}

#[tokio::test]
async fn budget_insert_failure_rolls_back_the_key_too() {
    let host = instance().await;
    host.handle().store().connection().execute_unprepared("CREATE TRIGGER reject_key_budget BEFORE INSERT ON quotas BEGIN SELECT RAISE(ABORT, 'test budget failure'); END").await.unwrap();
    let before = revision(&host).await;
    let result = create(&host, json!({"userId":"alice","name":"atomic-failed","budget":{"limitValue":"25.50","period":"1d"}})).await;
    assert!(!result.status.is_success());
    assert_eq!(revision(&host).await, before);
    let keys = host
        .send(keyed(
            get("/admin/api/api-keys?search=atomic-failed"),
            "root-key",
        ))
        .await;
    assert_eq!(keys.json()["total"], 0);
    let quotas = host.send(keyed(get("/admin/api/quotas"), "root-key")).await;
    assert_eq!(quotas.json()["total"], 0);
}

#[tokio::test]
async fn omitted_budget_preserves_creation_and_normal_users_cannot_set_operator_budgets() {
    let host = instance().await;
    let result = create(&host, json!({"userId":"alice","name":"plain"})).await;
    assert_eq!(result.status, StatusCode::OK);
    let quotas = host.send(keyed(get("/admin/api/quotas"), "root-key")).await;
    assert_eq!(quotas.json()["total"], 0);
    let result = host
        .send(keyed(
            post(
                "/admin/api/api-keys",
                json!({"userId":"alice","name":"forbidden","budget":{"limitValue":"100"}}),
            ),
            "alice-key",
        ))
        .await;
    assert_eq!(result.status, StatusCode::FORBIDDEN);
}
