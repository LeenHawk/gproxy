use std::collections::{BTreeMap, BTreeSet};

use super::super::{Executor, Statement};
use super::{libsql_store, native_store, scenario};
use crate::schema::{Dialect, tables};

#[tokio::test]
async fn native_and_libsql_share_schema_and_query_behavior() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    for path in [
        native_dir.path().join("native.db"),
        remote_dir.path().join("remote.db"),
    ] {
        let database = super::super::native::NativeSql::open(path).await.unwrap();
        crate::migration::migrate_to(
            &database,
            Dialect::NativeSqlite,
            crate::schema::SchemaVersion::Initial,
        )
        .await
        .unwrap();
    }
    let (native, native_db) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, remote_db) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");

    assert_eq!(schema_shape(native_db.as_ref()).await, expected_shape());
    assert_eq!(index_shape(native_db.as_ref()).await, expected_indexes());
    let versions = crate::schema::SchemaVersion::ALL
        .map(|version| version.number())
        .to_vec();
    assert_eq!(migration_versions(native_db.as_ref()).await, versions);
    assert_eq!(migration_versions(remote_db.as_ref()).await, versions);
    assert_eq!(
        schema_shape(native_db.as_ref()).await,
        schema_shape(remote_db.as_ref()).await
    );
    assert_eq!(
        index_shape(native_db.as_ref()).await,
        index_shape(remote_db.as_ref()).await
    );
    assert_eq!(scenario::run(&native).await, scenario::run(&libsql).await);
}

#[tokio::test]
async fn size_pressure_purges_logs_and_preserves_usage_history() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let (native, _) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, _) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");

    for store in [native, libsql] {
        scenario::run(&store).await;
        let result = store
            .cleanup_observability(None, Some(1))
            .await
            .expect("size-pressure sweep");
        assert!(result.over_size_limit);
        assert_eq!(result.pressure_rows, 3);
        assert_eq!(row_count(&store, "request_logs").await, 0);
        assert_eq!(row_count(&store, "wire_logs").await, 0);
        assert_eq!(row_count(&store, "usage_rows").await, 2);
    }
}

#[tokio::test]
async fn native_and_libsql_batch_failure_rolls_back() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let (_, native) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let (libsql, _) = libsql_store(remote_dir.path().join("remote.db"))
        .await
        .expect("libsql store");
    assert_batch_rollback(native.as_ref()).await;
    assert_batch_rollback(libsql.backend()).await;
}

#[tokio::test]
#[ignore = "requires empty PostgreSQL and MySQL databases via GPROXY_TEST_POSTGRES_DSN and GPROXY_TEST_MYSQL_DSN"]
async fn postgres_and_mysql_share_schema_queries_and_rollback() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let (native, _) = native_store(native_dir.path().join("native.db"))
        .await
        .expect("native store");
    let expected = scenario::run(&native).await;
    for (config, dialect) in [
        (
            crate::BackendConfig::Postgres {
                dsn: std::env::var("GPROXY_TEST_POSTGRES_DSN").expect("GPROXY_TEST_POSTGRES_DSN"),
            },
            Dialect::Postgres,
        ),
        (
            crate::BackendConfig::Mysql {
                dsn: std::env::var("GPROXY_TEST_MYSQL_DSN").expect("GPROXY_TEST_MYSQL_DSN"),
            },
            Dialect::Mysql,
        ),
    ] {
        let store = crate::Store::open(config).await.expect("SQL store");
        assert_eq!(
            table_names(&store, dialect).await,
            expected_shape().into_keys().collect()
        );
        assert_eq!(scenario::run(&store).await, expected);
        assert_batch_rollback(store.backend()).await;
    }
}

async fn assert_batch_rollback(executor: &dyn Executor) {
    executor
        .execute(Statement::plain(
            "DROP TABLE IF EXISTS gproxy_batch_rollback_test",
        ))
        .await
        .expect("drop rollback table");
    executor
        .execute(Statement::plain("CREATE TABLE gproxy_batch_rollback_test (id BIGINT PRIMARY KEY, value BIGINT NOT NULL)"))
        .await
        .expect("create rollback table");
    executor
        .execute(Statement::plain(
            "INSERT INTO gproxy_batch_rollback_test(id,value) VALUES(1,10)",
        ))
        .await
        .expect("seed rollback table");
    let result = executor
        .batch(vec![
            Statement::plain("UPDATE gproxy_batch_rollback_test SET value=20 WHERE id=1"),
            Statement::plain("INSERT INTO gproxy_missing_table(id) VALUES(1)"),
        ])
        .await;
    assert!(result.is_err());
    let result = executor
        .execute(Statement::plain(
            "SELECT value FROM gproxy_batch_rollback_test WHERE id=1",
        ))
        .await
        .expect("read rollback value");
    assert_eq!(result.rows[0].i64("value").expect("rollback value"), 10);
}

async fn table_names(store: &crate::Store, dialect: Dialect) -> BTreeSet<String> {
    let sql = match dialect {
        Dialect::Postgres => {
            "SELECT table_name AS name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE'"
        }
        Dialect::Mysql => {
            "SELECT table_name AS name FROM information_schema.tables WHERE table_schema=DATABASE() AND table_type='BASE TABLE'"
        }
        Dialect::NativeSqlite | Dialect::Libsql => unreachable!("live SQL dialect"),
    };
    store
        .backend()
        .execute(Statement::plain(sql))
        .await
        .expect("table catalog")
        .rows
        .into_iter()
        .map(|row| row.text("name").expect("table name").to_owned())
        .filter(|name| name != "gproxy_batch_rollback_test")
        .collect()
}

async fn row_count(store: &crate::Store, table: &str) -> i64 {
    store
        .backend()
        .execute(Statement::plain(format!(
            "SELECT COUNT(*) AS count FROM {table}"
        )))
        .await
        .expect("row count")
        .rows[0]
        .i64("count")
        .expect("count")
}

async fn migration_versions(executor: &dyn Executor) -> Vec<i64> {
    executor
        .execute(Statement::plain(
            "SELECT version FROM schema_migrations ORDER BY version",
        ))
        .await
        .expect("migration history")
        .rows
        .iter()
        .map(|row| row.i64("version").expect("migration version"))
        .collect()
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

async fn index_shape(executor: &dyn Executor) -> BTreeMap<String, (String, Vec<String>, bool)> {
    let indexes = executor
        .execute(Statement::plain(
            "SELECT name, tbl_name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL ORDER BY name",
        ))
        .await
        .expect("index catalog");
    let mut shape = BTreeMap::new();
    for row in indexes.rows {
        let name = row.text("name").expect("index name").to_owned();
        let table = row.text("tbl_name").expect("index table").to_owned();
        let unique = row
            .text("sql")
            .expect("index SQL")
            .starts_with("CREATE UNIQUE INDEX");
        let escaped = name.replace('"', "\"\"");
        let columns = executor
            .execute(Statement::plain(format!(
                "PRAGMA index_info(\"{escaped}\")"
            )))
            .await
            .expect("index columns")
            .rows
            .into_iter()
            .map(|row| row.text("name").expect("index column").to_owned())
            .collect();
        shape.insert(name, (table, columns, unique));
    }
    shape
}

fn expected_indexes() -> BTreeMap<String, (String, Vec<String>, bool)> {
    tables()
        .flat_map(|table| {
            table.indexes.iter().map(|index| {
                (
                    index.name.to_owned(),
                    (
                        table.name.to_owned(),
                        index
                            .columns
                            .iter()
                            .map(|column| (*column).into())
                            .collect(),
                        index.unique,
                    ),
                )
            })
        })
        .collect()
}
