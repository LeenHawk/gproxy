use std::collections::{BTreeMap, BTreeSet};

use super::super::{Executor, Statement};
use super::{libsql_store, native_store, scenario, store};
use crate::migration;
use crate::schema::{Dialect, SchemaVersion, tables};

#[tokio::test]
async fn native_and_libsql_share_schema_and_query_behavior() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let (native, native_db) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, remote_db) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");

    assert_eq!(schema_shape(native_db.as_ref()).await, expected_shape());
    assert_eq!(
        schema_shape(native_db.as_ref()).await,
        schema_shape(remote_db.as_ref()).await
    );
    assert_eq!(scenario::run(&native).await, scenario::run(&libsql).await);
}

#[tokio::test]
async fn fresh_and_incrementally_migrated_databases_converge() {
    let fresh_dir = tempfile::tempdir().expect("fresh tempdir");
    let old_dir = tempfile::tempdir().expect("old tempdir");
    let fresh = std::sync::Arc::new(
        super::super::native::NativeSql::open(fresh_dir.path().join("fresh.db"))
            .await
            .expect("fresh database"),
    );
    migration::migrate(fresh.as_ref(), Dialect::NativeSqlite)
        .await
        .expect("fresh migration");

    let old = std::sync::Arc::new(
        super::super::native::NativeSql::open(old_dir.path().join("old.db"))
            .await
            .expect("old database"),
    );
    migration::migrate_to(old.as_ref(), Dialect::NativeSqlite, SchemaVersion::Control)
        .await
        .expect("control migration");
    assert!(!schema_shape(old.as_ref()).await.contains_key("usage_rows"));
    migration::migrate(old.as_ref(), Dialect::NativeSqlite)
        .await
        .expect("runtime migration");

    assert_eq!(
        schema_shape(fresh.as_ref()).await,
        schema_shape(old.as_ref()).await
    );
    let fresh_store = store(fresh);
    let old_store = store(old);
    assert_eq!(
        scenario::run(&fresh_store).await,
        scenario::run(&old_store).await
    );
}

async fn schema_shape(executor: &dyn Executor) -> BTreeMap<String, BTreeSet<String>> {
    let tables = executor
        .execute(Statement::plain(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        ))
        .await
        .expect("table catalog");
    let mut shape = BTreeMap::new();
    for row in tables.rows {
        let table = row.text("name").expect("table name").to_owned();
        let escaped = table.replace('"', "\"\"");
        let columns = executor
            .execute(Statement::plain(format!(
                "PRAGMA table_info(\"{escaped}\")"
            )))
            .await
            .expect("column catalog")
            .rows
            .into_iter()
            .map(|row| row.text("name").expect("column name").to_owned())
            .collect();
        shape.insert(table, columns);
    }
    shape
}

fn expected_shape() -> BTreeMap<String, BTreeSet<String>> {
    let mut expected: BTreeMap<_, _> = tables()
        .map(|table| {
            (
                table.name.to_owned(),
                table
                    .columns
                    .iter()
                    .map(|column| column.name.to_owned())
                    .collect(),
            )
        })
        .collect();
    expected.insert(
        "schema_migrations".into(),
        ["version".into(), "applied_at".into()]
            .into_iter()
            .collect(),
    );
    expected
}
