#![cfg(not(target_arch = "wasm32"))]

use gproxy_store::{
    Store,
    entity::{config::setting, identity::user},
};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Set, Statement,
};
use serde_json::json;

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

#[tokio::test]
async fn sync_initializes_the_full_registry_and_is_repeatable() {
    let store = Store::new(connection().await);
    assert!(tables(store.connection()).await.is_empty());
    store.sync().await.unwrap();
    // The legacy one-shot API and portable sync share exactly the same registry.
    let reference = connection().await;
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&reference)
        .await
        .unwrap();
    assert_eq!(tables(store.connection()).await, tables(&reference).await);
    assert!(store.settings().get().await.unwrap().is_none());
    store
        .settings()
        .update(setting::ActiveModel {
            max_attempts: Set(7),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("existing".into()),
            name: Set("kept".into()),
            role: Set("user".into()),
            created_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
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

#[tokio::test]
async fn sync_adds_allowlist_columns_to_populated_schema_without_losing_data() {
    let store = Store::new(connection().await);
    store.sync().await.unwrap();
    store
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set("existing".into()),
            name: Set("kept".into()),
            role: Set("user".into()),
            created_at_ms: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .settings()
        .update(setting::ActiveModel {
            max_attempts: Set(7),
            ..Default::default()
        })
        .await
        .unwrap();
    // Model the immediately previous schema, before the four nullable policy columns.
    for table in ["settings", "organizations", "teams", "users"] {
        store
            .connection()
            .execute_unprepared(&format!(
                "ALTER TABLE {table} DROP COLUMN oauth_client_allowlist"
            ))
            .await
            .unwrap();
    }
    store
        .connection()
        .execute_unprepared("CREATE TABLE legacy_data (value TEXT NOT NULL)")
        .await
        .unwrap();
    store
        .connection()
        .execute_unprepared("INSERT INTO legacy_data VALUES ('retained')")
        .await
        .unwrap();
    store.sync().await.unwrap();
    let user = store
        .users()
        .get_many(&["existing".into()])
        .await
        .unwrap()
        .remove(0)
        .unwrap();
    assert_eq!(user.name, "kept");
    assert!(user.oauth_client_allowlist.is_none());
    let settings = store.settings().get().await.unwrap().unwrap();
    assert_eq!(settings.max_attempts, 7);
    assert!(settings.oauth_client_allowlist.is_none());
    store.load_all_data().await.unwrap();
    store
        .settings()
        .update(setting::ActiveModel {
            oauth_client_allowlist: Set(Some(json!(["client"]))),
            ..Default::default()
        })
        .await
        .unwrap();
    store.sync().await.unwrap();
    assert_eq!(
        store
            .settings()
            .get()
            .await
            .unwrap()
            .unwrap()
            .oauth_client_allowlist,
        Some(json!(["client"]))
    );
    let row = store
        .connection()
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT value FROM legacy_data",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<String>("", "value").unwrap(), "retained");
}

/// The upgrade path for the API-key binding and the audit trail: an existing
/// database gains the two nullable columns and the new table in place.
#[tokio::test]
async fn sync_adds_the_api_key_bindings_and_the_audit_table_in_place() {
    use gproxy_store::entity::identity::{api_key, organization};
    let store = Store::new(connection().await);
    store.sync().await.unwrap();
    // Model the immediately previous schema, before the binding and the table.
    // SQLite refuses DROP COLUMN on a column named by a foreign key, so the
    // previous api_keys definition is restated instead.
    for statement in [
        "DROP TABLE api_keys",
        "CREATE TABLE \"api_keys\" ( \"id\" varchar NOT NULL PRIMARY KEY, \
         \"user_id\" varchar NOT NULL, \"subscription_id\" varchar, \"name\" varchar NOT NULL, \
         \"kind\" varchar(16) NOT NULL DEFAULT 'user', \"key_hash\" varchar NOT NULL UNIQUE, \
         \"prefix\" varchar NOT NULL, \"secret\" varbinary_blob, \"expires_at_ms\" integer, \
         \"enabled\" boolean NOT NULL DEFAULT TRUE, \
         FOREIGN KEY (\"user_id\") REFERENCES \"users\" (\"id\") ON DELETE CASCADE, \
         FOREIGN KEY (\"subscription_id\") REFERENCES \"subscriptions\" (\"id\") ON DELETE CASCADE )",
        "DROP TABLE audit_events",
    ] {
        store
            .connection()
            .execute_unprepared(statement)
            .await
            .unwrap();
    }
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
        .connection()
        .execute_unprepared(
            "INSERT INTO api_keys (id, user_id, name, kind, key_hash, prefix, enabled) \
             VALUES ('k', 'u', 'k', 'user', 'hash', 'sk-', 1)",
        )
        .await
        .unwrap();
    assert!(
        !tables(store.connection())
            .await
            .contains(&"audit_events".into())
    );
    store.sync().await.unwrap();
    assert!(
        tables(store.connection())
            .await
            .contains(&"audit_events".into())
    );
    // The pre-existing key is retained and simply unbound.
    let key = store.load_identity_data().await.unwrap().api_keys.remove(0);
    assert_eq!(key.key_hash, "hash");
    assert!(key.organization_id.is_none() && key.team_id.is_none());
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
        .api_keys()
        .update_many(vec![api_key::ActiveModel {
            id: Set("k".into()),
            organization_id: Set(Some("org".into())),
            ..Default::default()
        }])
        .await
        .unwrap();
    assert_eq!(
        store.load_identity_data().await.unwrap().api_keys[0]
            .organization_id
            .as_deref(),
        Some("org")
    );
    // Documented limitation of in-place column addition: SQLite's ALTER TABLE
    // ADD COLUMN leaves the table's foreign keys untouched, so an upgraded
    // database gains the columns without the Cascade a freshly created one has.
    // The application layer must therefore unbind keys itself when it deletes a
    // scope, rather than relying on the database to do it.
    let ddl: String = store
        .connection()
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'api_keys'",
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get("", "sql")
        .unwrap();
    assert!(!ddl.contains("REFERENCES \"organizations\""), "{ddl}");
}
