use std::collections::{BTreeMap, BTreeSet};

use super::super::libsql::LibsqlHttp;
use super::super::native::NativeSql;
use super::super::{Executor, SharedExecutor, Statement};
use super::sender::SqliteHrana;
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
    assert_eq!(index_shape(native_db.as_ref()).await, expected_indexes());
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
    assert_eq!(
        index_shape(fresh.as_ref()).await,
        index_shape(old.as_ref()).await
    );
    let fresh_store = store(fresh);
    let old_store = store(old);
    assert_eq!(
        scenario::run(&fresh_store).await,
        scenario::run(&old_store).await
    );
}

#[tokio::test]
async fn wave_30_preserves_vocabularies_and_backfills_their_repository() {
    let directory = tempfile::tempdir().expect("native tempdir");
    let executor = std::sync::Arc::new(
        NativeSql::open(directory.path().join("tokenizers.db"))
            .await
            .expect("native database"),
    );
    migration::migrate_to(
        executor.as_ref(),
        Dialect::NativeSqlite,
        SchemaVersion::Wave29,
    )
    .await
    .expect("pre-wave migration");
    executor
        .execute(Statement::plain(
            "INSERT INTO tokenizer_vocabs (name, bytes, updated_at) VALUES ('owner/model', X'0102', 100)",
        ))
        .await
        .expect("seed tokenizer");

    migration::migrate(executor.as_ref(), Dialect::NativeSqlite)
        .await
        .expect("wave 30 migration");
    let vocab = store(executor)
        .tokenizer_vocabs()
        .await
        .expect("list tokenizers")
        .pop()
        .expect("preserved tokenizer");

    assert_eq!(vocab.name, "owner/model");
    assert_eq!(vocab.repository, "owner/model");
}

#[tokio::test]
async fn wave_26_preserves_admin_identity_across_backends() {
    let native_dir = tempfile::tempdir().expect("native tempdir");
    let remote_dir = tempfile::tempdir().expect("libsql tempdir");
    let native: SharedExecutor = std::sync::Arc::new(
        NativeSql::open(native_dir.path().join("native.db"))
            .await
            .expect("native database"),
    );
    let remote = std::sync::Arc::new(
        NativeSql::open(remote_dir.path().join("remote.db"))
            .await
            .expect("remote database"),
    );
    let libsql: SharedExecutor = std::sync::Arc::new(LibsqlHttp::with_sender(
        "https://store.invalid".into(),
        "test-token".into(),
        SqliteHrana::new(remote),
    ));

    for (executor, dialect) in [(native, Dialect::NativeSqlite), (libsql, Dialect::Libsql)] {
        migration::migrate_to(executor.as_ref(), dialect, SchemaVersion::Configuration)
            .await
            .expect("pre-wave migration");
        executor
            .batch(vec![
                Statement::plain("INSERT INTO organizations (id, name, enabled) VALUES (1, 'existing', 1)"),
                Statement::plain("INSERT INTO users (id, name, organization_id, team_id, enabled) VALUES (1, 'existing-user', 1, NULL, 1)"),
                Statement::plain("INSERT INTO admin_accounts (id, username, password_hash, enabled, created_at) VALUES (1, 'operator', 'argon2-hash', 1, 100)"),
                Statement::plain("INSERT INTO admin_sessions (token_digest, admin_id, created_at, expires_at) VALUES (X'0102', 1, 100, 200)"),
                Statement::plain("INSERT INTO admin_api_keys (digest, admin_id, created_at) VALUES (X'0304', 1, 100)"),
                Statement::plain("INSERT INTO admin_audit_events (actor_admin_id, action, target_kind, target_id, at, details_json) VALUES (1, 'provider.update', 'provider', 9, 110, NULL)"),
                Statement::plain("INSERT INTO credential_health (credential_id, credential_version, version, state, observed_at, response_status, detail) VALUES (7, 1, 2, 'dead', 120, 401, 'rejected')"),
            ])
            .await
            .expect("pre-wave seed");

        migration::migrate(executor.as_ref(), dialect)
            .await
            .expect("wave 26 migration");
        assert_eq!(schema_shape(executor.as_ref()).await, expected_shape());
        assert_eq!(index_shape(executor.as_ref()).await, expected_indexes());

        let store = crate::Store {
            executor: executor.clone(),
            dialect,
        };
        let admin = store
            .admin_by_username("operator")
            .await
            .expect("admin lookup")
            .expect("migrated admin");
        assert_eq!(admin.password_hash, "argon2-hash");
        assert_ne!(
            admin.id, 1,
            "legacy admin id must not be copied as a user id"
        );
        assert_eq!(
            store
                .admin_for_session(&[1, 2], 150)
                .await
                .expect("session lookup")
                .expect("migrated session")
                .id,
            admin.id
        );
        assert_eq!(
            store
                .admin_for_api_key(&[3, 4], 150)
                .await
                .expect("key lookup")
                .expect("migrated key")
                .id,
            admin.id
        );
        let audit = store.audit_events(1).await.expect("audit lookup");
        assert_eq!(audit[0].event.actor_user_id, admin.id);
        let snapshot = store.control_snapshot().await.expect("control snapshot");
        let user = snapshot
            .users
            .iter()
            .find(|user| user.id == admin.id)
            .expect("admin in user list");
        assert!(user.is_admin);
        assert!(user.organization_id.is_some());
        assert!(snapshot.permissions.iter().any(|permission| {
            permission.subject_kind == "user"
                && permission.subject_id == admin.id
                && permission.allowed
        }));
        let health = store.credential_health().await.expect("health lookup");
        assert_eq!(health[0].model, "*");
    }
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
