#![cfg(not(target_arch = "wasm32"))]
//! `sync` is the name the sdk and the hosts call the schema step by, and what
//! it means changed: it creates the schema on an empty database, applies
//! outstanding migrations on one this build created, and refuses anything else.
//! What it no longer does — work out the difference between a populated
//! database and the entity registry, and emit the `ALTER`s — is what these tests
//! now pin down, because getting it back by accident is the failure mode.

use gproxy_store::{
    SchemaState, Store,
    entity::{config::setting, identity::user},
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Set, Statement,
};

async fn connection() -> DatabaseConnection {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    Database::connect(options).await.unwrap()
}

async fn tables(db: &DatabaseConnection) -> Vec<String> {
    db.query_all_raw(Statement::from_string(DbBackend::Sqlite,
        "SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"))
        .await.unwrap().into_iter().map(|r| r.try_get("", "name").unwrap()).collect()
}

async fn a_user(store: &Store<DatabaseConnection>, id: &str) {
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set(id.into()),
            name: Set("kept".into()),
            role: Set("user".into()),
            created_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
}

#[tokio::test]
async fn sync_initializes_the_full_registry_and_is_repeatable() {
    let store = Store::new(connection().await);
    assert!(tables(store.connection()).await.is_empty());
    store.sync().await.unwrap();

    // The one-shot registry apply and the schema step agree on every table, and
    // the schema step adds the one thing it owns beyond them: its ledger.
    let reference = connection().await;
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&reference)
        .await
        .unwrap();
    let mut expected = tables(&reference).await;
    expected.push(gproxy_store::MIGRATION_LEDGER.into());
    expected.sort();
    assert_eq!(tables(store.connection()).await, expected);

    assert!(store.settings().get().await.unwrap().is_none());
    store
        .settings()
        .update(setting::ActiveModel {
            max_attempts: Set(7),
            ..Default::default()
        })
        .await
        .unwrap();
    a_user(&store, "existing").await;
    store.sync().await.unwrap();
    store.sync().await.unwrap();
    assert_eq!(
        store.settings().get().await.unwrap().unwrap().max_attempts,
        7
    );
    assert_eq!(
        store.users().get_many(&["existing".into()]).await.unwrap()[0]
            .as_ref()
            .unwrap()
            .name,
        "kept"
    );
    assert_eq!(store.load_identity_data().await.unwrap().users.len(), 1);
}

/// The behaviour that had to go. A column removed behind the migrator's back is
/// not quietly put back: the ledger says this database is where this build
/// expects it to be, so `sync` has nothing to do and says so. Repairing it is a
/// migration's job, and a migration is a thing somebody writes.
#[tokio::test]
async fn sync_does_not_invent_ddl_for_a_database_that_drifted() {
    let store = Store::new(connection().await);
    store.sync().await.unwrap();
    a_user(&store, "existing").await;
    store
        .connection()
        .execute_unprepared("ALTER TABLE users DROP COLUMN oauth_client_allowlist")
        .await
        .unwrap();

    let report = store.sync().await.unwrap();
    assert!(!report.installed);
    assert!(report.applied.is_empty(), "{report:?}");

    let error = store
        .users()
        .get_many(&["existing".into()])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("oauth_client_allowlist"),
        "{error}"
    );
}

/// Likewise for a table: `sync` does not put back what it did not create, and a
/// database that lost one is a database somebody has to repair deliberately.
#[tokio::test]
async fn sync_does_not_recreate_a_dropped_table() {
    let store = Store::new(connection().await);
    store.sync().await.unwrap();
    store
        .connection()
        .execute_unprepared("DROP TABLE audit_events")
        .await
        .unwrap();
    store.sync().await.unwrap();
    assert!(
        !tables(store.connection())
            .await
            .contains(&"audit_events".into())
    );
}

/// The failure this whole arrangement came from: a database whose schema this
/// build does not own. It is refused whole, and the refusal names both ledgers
/// so an operator can tell which product's database they pointed at.
#[tokio::test]
async fn sync_refuses_a_database_it_did_not_create() {
    let store = Store::new(connection().await);
    store
        .connection()
        .execute_unprepared("CREATE TABLE users (id integer NOT NULL PRIMARY KEY)")
        .await
        .unwrap();
    store
        .connection()
        .execute_unprepared("CREATE TABLE schema_migrations (version varchar NOT NULL)")
        .await
        .unwrap();
    let before = tables(store.connection()).await;

    let error = store.sync().await.unwrap_err().to_string();
    assert!(error.contains(gproxy_store::MIGRATION_LEDGER), "{error}");
    assert!(error.contains("schema_migrations"), "{error}");
    assert_eq!(tables(store.connection()).await, before);
    assert!(matches!(
        store.schema_state().await.unwrap(),
        SchemaState::Foreign { .. }
    ));
}
