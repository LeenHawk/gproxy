#![cfg(not(target_arch = "wasm32"))]
//! `commit_revision`: one bump per call, atomic with the caller's statements.

use gproxy_seaorm::BatchStatement;
use gproxy_store::{
    Store,
    entity::{config::setting, upstream::provider},
};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;

async fn database() -> Store<DatabaseConnection> {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    // The singleton must exist before any revision can be advanced.
    store
        .settings()
        .update(setting::ActiveModel {
            instance_name: Set("test".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    store
}

fn provider(id: &str) -> provider::ActiveModel {
    provider::ActiveModel {
        id: Set(id.into()),
        name: Set(id.into()),
        channel: Set("test".into()),
        config: Set(json!({})),
        created_at_ms: Set(0),
        ..Default::default()
    }
}

async fn revision(store: &Store<DatabaseConnection>) -> i64 {
    store
        .settings()
        .get()
        .await
        .unwrap()
        .unwrap()
        .config_revision
}

#[tokio::test]
async fn every_commit_advances_the_revision_by_exactly_one_and_returns_it() {
    let store = database().await;
    assert_eq!(revision(&store).await, 0);

    let first = store
        .commit_revision(vec![BatchStatement::Execute(
            store.providers().insert_statement(provider("p1")).unwrap(),
        )])
        .await
        .unwrap();
    assert_eq!(first.revision, 1);
    assert_eq!(first.results.len(), 1, "one result per caller statement");
    assert_eq!(revision(&store).await, 1);

    // Several statements are still one revision, and an empty commit is a
    // legitimate way to make peers reload.
    let second = store
        .commit_revision(vec![
            BatchStatement::Execute(store.providers().insert_statement(provider("p2")).unwrap()),
            BatchStatement::Execute(store.providers().insert_statement(provider("p3")).unwrap()),
        ])
        .await
        .unwrap();
    assert_eq!(second.revision, 2);
    assert_eq!(second.results.len(), 2);
    let third = store.commit_revision(Vec::new()).await.unwrap();
    assert_eq!(third.revision, 3);
    assert!(third.results.is_empty());
    assert_eq!(revision(&store).await, 3);
    assert_eq!(store.load_control_data().await.unwrap().providers.len(), 3);
}

#[tokio::test]
async fn a_failing_statement_rolls_the_bump_back_with_it() {
    let store = database().await;
    store
        .commit_revision(vec![BatchStatement::Execute(
            store.providers().insert_statement(provider("p1")).unwrap(),
        )])
        .await
        .unwrap();
    assert_eq!(revision(&store).await, 1);

    // The second insert collides on the primary key; nothing in the batch stands.
    let outcome = store
        .commit_revision(vec![
            BatchStatement::Execute(store.providers().insert_statement(provider("p2")).unwrap()),
            BatchStatement::Execute(store.providers().insert_statement(provider("p1")).unwrap()),
        ])
        .await;
    assert!(outcome.is_err(), "duplicate key must fail the batch");
    assert_eq!(revision(&store).await, 1, "no bump without a write");
    assert_eq!(store.load_control_data().await.unwrap().providers.len(), 1);
}

#[tokio::test]
async fn a_settings_write_can_ride_along_in_the_same_revision() {
    let store = database().await;
    let commit = store
        .commit_revision(vec![BatchStatement::Execute(
            store
                .settings()
                .update_statement(setting::ActiveModel {
                    max_attempts: Set(9),
                    ..Default::default()
                })
                .unwrap(),
        )])
        .await
        .unwrap();
    assert_eq!(commit.revision, 1);
    let settings = store.settings().get().await.unwrap().unwrap();
    assert_eq!(settings.max_attempts, 9);
    assert_eq!(settings.config_revision, 1);
}

#[tokio::test]
async fn a_missing_settings_row_is_refused() {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    assert!(store.commit_revision(Vec::new()).await.is_err());
}
