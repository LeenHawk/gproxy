#![cfg(not(target_arch = "wasm32"))]
//! API-key organization/team binding and the audit trail.

use gproxy_store::{
    Store,
    entity::identity::{api_key, audit_event, organization, team, user},
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, EntityTrait,
    QueryOrder, Set, Statement,
};
use serde_json::json;

async fn store() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    Store::new(db)
}

/// One organization, one team inside it, one user and one key bound to both.
async fn bound() -> Store<DatabaseConnection> {
    let store = store().await;
    store
        .organizations()
        .create_many(vec![organization::ActiveModel {
            id: Set("org".into()),
            name: Set("org".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .teams()
        .create_many(vec![team::ActiveModel {
            id: Set("team".into()),
            organization_id: Set("org".into()),
            name: Set("team".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("u".into()),
            name: Set("u".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("k".into()),
            user_id: Set("u".into()),
            organization_id: Set(Some("org".into())),
            team_id: Set(Some("team".into())),
            name: Set("k".into()),
            key_hash: Set("hash".into()),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
}

/// The binding is what decides the budget owner chain, the permission subject
/// and the credential-visibility boundary, so it has to reach the application
/// layer through the identity snapshot, not through a separate read.
#[tokio::test]
async fn load_identity_data_carries_the_api_key_bindings() {
    let store = bound().await;
    let identity = store.load_identity_data().await.unwrap();
    let key = &identity.api_keys[0];
    assert_eq!(key.organization_id.as_deref(), Some("org"));
    assert_eq!(key.team_id.as_deref(), Some("team"));
    assert_eq!(key.user_id, "u");
    // The same rows arrive on the single-batch read used by hosts.
    let all = store.load_all_data().await.unwrap();
    assert_eq!(all.identity.api_keys, identity.api_keys);
    // An unbound key keeps both columns empty rather than defaulting to a scope.
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("free".into()),
            user_id: Set("u".into()),
            name: Set("free".into()),
            key_hash: Set("other".into()),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    let identity = store.load_identity_data().await.unwrap();
    let free = identity.api_keys.iter().find(|k| k.id == "free").unwrap();
    assert!(free.organization_id.is_none() && free.team_id.is_none());
}

/// Observed behaviour, with SQLite foreign keys enforced: both bindings are
/// `on_delete = Cascade`, so deleting the scope deletes the keys bound to it
/// rather than leaving a key whose owner chain and visibility boundary are
/// gone. Deleting a team does not touch keys bound only to its organization.
#[tokio::test]
async fn deleting_a_scope_cascades_to_the_keys_bound_to_it() {
    let store = bound().await;
    assert_eq!(
        store
            .connection()
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_keys",
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i32>("", "foreign_keys")
            .unwrap(),
        1,
        "the cascade assertions below are only meaningful with FKs enforced"
    );
    store
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set("org-only".into()),
            user_id: Set("u".into()),
            organization_id: Set(Some("org".into())),
            name: Set("org-only".into()),
            key_hash: Set("org-hash".into()),
            prefix: Set("sk-".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store.teams().delete_many(&["team".into()]).await.unwrap(),
        [1]
    );
    let keys = store.load_identity_data().await.unwrap().api_keys;
    assert_eq!(
        keys.iter().map(|k| k.id.as_str()).collect::<Vec<_>>(),
        ["org-only"],
        "the team-bound key is deleted, the organization-bound key is not"
    );
    // Deleting the organization removes the remaining key and the team rows with it.
    store
        .organizations()
        .delete_many(&["org".into()])
        .await
        .unwrap();
    let identity = store.load_identity_data().await.unwrap();
    assert!(identity.api_keys.is_empty() && identity.teams.is_empty());
    assert_eq!(identity.users.len(), 1, "the user itself is untouched");
}

/// Audit rows outlive their actors, so the table carries no foreign keys.
#[tokio::test]
async fn audit_events_have_no_foreign_keys() {
    let store = store().await;
    let ddl: String = store
        .connection()
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'audit_events'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "sql")
        .unwrap();
    assert!(
        !ddl.to_uppercase().contains("FOREIGN KEY"),
        "audit_events must survive actor deletion: {ddl}"
    );
    store
        .audit_events()
        .create_many(vec![audit_event::ActiveModel {
            id: Set("a".into()),
            actor_user_id: Set(Some("deleted-user".into())),
            actor_api_key_id: Set(Some("deleted-key".into())),
            entity_id: Set(Some("gone".into())),
            action: Set("providers.create".into()),
            outcome: Set(audit_event::OUTCOME_OK.into()),
            detail: Set(json!({})),
            created_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store.audit_events().get_many(&["a".into()]).await.unwrap()[0]
            .as_ref()
            .unwrap()
            .actor_user_id
            .as_deref(),
        Some("deleted-user")
    );
}

#[tokio::test]
async fn audit_events_page_newest_first() {
    let store = store().await;
    let rows = (0..5)
        .map(|i| audit_event::ActiveModel {
            id: Set(format!("e{i}")),
            actor_user_id: Set(Some("admin".into())),
            source_ip: Set(Some("10.0.0.1".into())),
            action: Set("providers.create".into()),
            entity_kind: Set(Some("provider".into())),
            entity_id: Set(Some(format!("p{i}"))),
            outcome: Set(if i == 4 {
                audit_event::OUTCOME_ERROR.into()
            } else {
                audit_event::OUTCOME_OK.into()
            }),
            detail: Set(json!({ "name": format!("p{i}"), "secret": "[redacted]" })),
            created_at_ms: Set(100 + i),
            ..Default::default()
        })
        .collect();
    store.audit_events().create_many(rows).await.unwrap();
    let newest_first = audit_event::Entity::find().order_by_desc(audit_event::Column::CreatedAtMs);
    let page = store
        .audit_events()
        .page(newest_first.clone(), 0, 2)
        .await
        .unwrap();
    assert_eq!(page.total, 5);
    assert_eq!(
        page.items.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["e4", "e3"]
    );
    assert_eq!(page.items[0].outcome, audit_event::OUTCOME_ERROR);
    assert_eq!(page.items[0].detail["secret"], json!("[redacted]"));
    let next = store.audit_events().page(newest_first, 2, 2).await.unwrap();
    assert_eq!(
        next.items.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["e2", "e1"]
    );
    assert_eq!((next.total, next.offset, next.limit), (5, 2, 2));
}
