#![cfg(not(target_arch = "wasm32"))]
mod support;
use gproxy_store::entity::{
    identity::audit_event,
    usage::{capture_record, usage_record},
};
use http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use support::{Host, Reply, get, keyed, post};

async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    for (user, role) in [("root", "admin"), ("alice", "user"), ("bob", "user")] {
        support::person(&handle, user, role).await;
        support::api_key(&handle, &format!("k-{user}"), user, None, None).await;
        support::allow(&handle, &format!("allow-{user}"), user, None).await;
    }
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    // The logs these tests read are off until an operator turns them on.
    handle
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_downstream_log: Some(true),
                enable_upstream_log: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    host.publish().await;
    host
}

#[tokio::test]
async fn global_usage_and_logs_are_instance_only_and_personal_usage_stays_scoped() {
    let host = instance().await;
    let store = host.app.gproxy().store();
    store
        .usage_records()
        .create_many(
            [("a", "alice", 3), ("b", "bob", 7)]
                .into_iter()
                .map(|(id, user, tokens)| usage_record::ActiveModel {
                    request_id: Set(id.into()),
                    user_id: Set(Some(user.into())),
                    api_key_id: Set(Some(format!("k-{user}"))),
                    model: Set("m1".into()),
                    operation: Set("generate_content".into()),
                    input_tokens: Set(Some(tokens)),
                    state: Set(Some("settled".into())),
                    metrics: Set(json!({})),
                    started_at_ms: Set(100),
                    ended_at_ms: Set(Some(101)),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    let global = host
        .send(keyed(
            get("/admin/api/usage?fromMs=0&toMs=200&groupBy=user&bucketMs=100"),
            "k-root",
        ))
        .await;
    assert_eq!(global.status, StatusCode::OK, "{}", global.text());
    assert_eq!(global.json()["summary"]["inputTokens"], 10);
    assert_eq!(global.json()["groups"].as_array().unwrap().len(), 2);
    assert_eq!(global.json()["trend"].as_array().unwrap().len(), 2);
    let personal = host
        .send(keyed(get("/portal/api/usage?userId=bob"), "k-alice"))
        .await;
    assert_eq!(personal.json()["summary"]["inputTokens"], 3);
    for path in [
        "/admin/api/usage",
        "/admin/api/usage/records",
        "/admin/api/logs/downstream",
        "/admin/api/logs/upstream",
        "/admin/api/logs/captures/a",
        "/admin/api/logs/downstream/a",
    ] {
        let denied = host.send(keyed(get(path), "k-alice")).await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN, "{path}");
    }
    let page = host
        .send(keyed(
            get("/admin/api/usage/records?userId=bob&pageSize=1"),
            "k-root",
        ))
        .await;
    assert_eq!(page.json()["total"], 1);
    assert_eq!(page.json()["items"][0]["userId"], "bob");
}

#[tokio::test]
async fn upstream_cursor_and_detail_work_without_a_downstream_record() {
    let host = instance().await;
    host.app
        .gproxy()
        .store()
        .capture_records()
        .create_many(
            ["a", "b", "c"]
                .into_iter()
                .map(|id| capture_record::ActiveModel {
                    id: Set(id.into()),
                    initiator_request_id: Set(Some("parent".into())),
                    side: Set(capture_record::CaptureSide::Upstream),
                    kind: Set(capture_record::CaptureKind::Http),
                    provider_id: Set(Some("p1".into())),
                    started_at_ms: Set(100),
                    ..Default::default()
                })
                .collect(),
        )
        .await
        .unwrap();
    let first = host
        .send(keyed(
            get("/admin/api/logs/upstream?limit=2&providerId=p1"),
            "k-root",
        ))
        .await
        .json();
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let next = format!(
        "/admin/api/logs/upstream?limit=2&cursor={}&cursorId={}",
        first["nextCursor"],
        first["nextCursorId"].as_str().unwrap()
    );
    let second = host.send(keyed(get(&next), "k-root")).await.json();
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        first["items"][0]["requestId"],
        second["items"][0]["requestId"]
    );
    assert!(second["nextCursor"].is_null());
    let detail = host
        .send(keyed(get("/admin/api/logs/captures/a"), "k-root"))
        .await;
    assert_eq!(detail.status, StatusCode::OK);
    assert_eq!(detail.json()["record"]["initiatorRequestId"], "parent");
    let down = host
        .send(keyed(get("/admin/api/logs/downstream"), "k-root"))
        .await;
    assert!(down.json()["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn channel_requests_and_services_are_logs_while_management_reads_are_audited() {
    let host = instance().await;
    host.client
        .script(vec![Reply::Http(StatusCode::OK, json!({"ok":true}))]);
    assert_eq!(
        host.send(keyed(
            post("/v1/messages", json!({"model":"test/m1"})),
            "k-alice"
        ))
        .await
        .status,
        StatusCode::OK
    );
    assert_eq!(
        host.send(keyed(get("/p1/backend-api/wham/usage"), "k-alice"))
            .await
            .status,
        StatusCode::OK
    );
    let store = host.app.gproxy().store();
    assert!(
        store
            .audit_events()
            .query(audit_event::Entity::find())
            .await
            .unwrap()
            .is_empty()
    );
    let rows = store
        .capture_records()
        .query(capture_record::Entity::find())
        .await
        .unwrap();
    let services: Vec<_> = rows
        .iter()
        .filter(|r| r.operation.as_deref() == Some("service"))
        .collect();
    assert_eq!(services.len(), 1);
    assert_eq!(services[0].user_id.as_deref(), Some("alice"));
    assert_eq!(services[0].state, capture_record::CaptureState::Completed);
    assert_eq!(
        store
            .usage_records()
            .query(
                usage_record::Entity::find().filter(
                    usage_record::Column::Side
                        .eq(gproxy_store::entity::usage::capture_record::CaptureSide::Downstream)
                )
            )
            .await
            .unwrap()
            .len(),
        1,
        "service did not meter model usage"
    );
    let logs = host
        .send(keyed(get("/admin/api/logs/downstream"), "k-root"))
        .await
        .json();
    assert_eq!(logs["items"].as_array().unwrap().len(), 2);
    let model_request = rows
        .iter()
        .find(|r| {
            r.side == capture_record::CaptureSide::Downstream
                && r.operation.as_deref() != Some("service")
        })
        .unwrap();
    let detail = host
        .send(keyed(
            get(&format!("/admin/api/logs/downstream/{}", model_request.id)),
            "k-root",
        ))
        .await
        .json();
    assert_eq!(detail["upstream"].as_array().unwrap().len(), 1);
    assert_eq!(detail["usage"]["userId"], "alice");
    let filtered = host
        .send(keyed(
            get("/admin/api/logs/downstream?credentialId=c1"),
            "k-root",
        ))
        .await;
    assert_eq!(
        filtered.json()["items"].as_array().unwrap().len(),
        1,
        "filter traverses upstream links"
    );
    let audit = store
        .audit_events()
        .query(audit_event::Entity::find())
        .await
        .unwrap();
    assert_eq!(audit.len(), 3, "management reads are audited once each");
    assert!(
        audit
            .iter()
            .all(|r| r.detail["method"] == "GET" && r.actor_user_id.as_deref() == Some("root"))
    );
}

#[tokio::test]
async fn sign_in_is_attributed_to_the_new_account_and_never_retains_the_password() {
    let host = instance().await;
    let data = host.data();
    host.operations(&data)
        .users()
        .set_password("alice", "correct horse battery")
        .await
        .unwrap();
    drop(data);
    host.publish().await;
    let answer = host
        .send(post(
            "/portal/api/login",
            json!({"name":"alice","password":"correct horse battery"}),
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK);
    let rows = host
        .app
        .gproxy()
        .store()
        .audit_events()
        .query(audit_event::Entity::find())
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].actor_user_id.as_deref(), Some("alice"));
    assert!(!rows[0].detail.to_string().contains("correct horse battery"));
    assert!(
        !rows[0]
            .detail
            .to_string()
            .contains(answer.json()["token"].as_str().unwrap())
    );
}
